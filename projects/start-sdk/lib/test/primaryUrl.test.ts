import { mkdtempSync, readFileSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { Effects } from '@start9labs/start-core/Effects'
import * as T from '@start9labs/start-core/types'
import { z } from '@start9labs/start-core/zExport'
import { FileHelper } from '../util/fileHelper'
import { sdk } from './output.sdk'

const dir = mkdtempSync(join(tmpdir(), 'primary-url-'))
const tick = () => new Promise(r => setTimeout(r, 50))
const shape = z.looseObject({ primaryUrl: z.string().optional() })

const row = (
  hostname: string,
  metadata: T.HostnameMetadata,
  port = 8080,
  ssl = false,
): T.HostnameInfo => ({ ssl, public: false, hostname, port, metadata })
const lan = row('192.168.1.10', { kind: 'ipv4', gateway: 'eth0' })
const local = row('box.local', { kind: 'mdns', gateways: ['eth0'] })
const onion = row('abc.onion', {
  kind: 'plugin',
  packageId: 'tor',
  removeAction: null,
  overflowActions: [],
  info: null,
})
const bridge = row('10.0.3.1', { kind: 'ipv4', gateway: 'lxcbr0' })

const host = (available: T.HostnameInfo[]): T.Host => ({
  bindings: {
    80: {
      enabled: true,
      options: { preferredExternalPort: 80, addSsl: null, secure: null },
      net: { assignedPort: 8080, assignedSslPort: 443 },
      addresses: {
        enabled: [],
        disabled: [],
        guaWan: [],
        lanEnabled: [],
        available,
      },
      interfaces: {
        ui: {
          id: 'ui',
          name: 'UI',
          description: '',
          masked: false,
          type: 'ui',
          addressInfo: {
            username: null,
            hostId: 'ui-multi',
            internalPort: 80,
            scheme: 'http',
            sslScheme: 'https',
            suffix: '',
          },
        },
      },
    },
  },
  bindingRanges: {},
  publicDomains: {},
  privateDomains: {},
  portForwards: [],
})

/** The container runtime's `CallbackHolder`: a same-named child replaces its predecessor, and a holder that left context has its callbacks dropped. */
class Holder {
  children = new Map<string, Holder>()
  leaveFns: (() => void)[] = []
  left = false
  child(name: string) {
    this.children.get(name)?.leave()
    const child = new Holder()
    this.children.set(name, child)
    return child
  }
  leave() {
    for (const child of this.children.values()) child.leave()
    this.children = new Map()
    this.left = true
    for (const fn of this.leaveFns) fn()
    this.leaveFns = []
  }
}

const setup = (available: T.HostnameInfo[] | null, chosen?: string) => {
  const file = FileHelper.json(
    join(mkdtempSync(join(dir, 'case-')), 'store.json'),
    shape,
  )
  if (chosen) writeFileSync(file.path, JSON.stringify({ primaryUrl: chosen }))
  let rows = available
  const hostCallbacks: { holder: Holder; callback: () => void }[] = []
  const createTask = jest.fn(async (_: unknown) => null)
  const set = jest.fn((effects: T.Effects, url: string) =>
    file.merge(effects, { primaryUrl: url }),
  )
  const makeEffects = (
    constRetry?: () => void,
    holder = new Holder(),
  ): Effects => {
    const effects = {
      eventId: 'event',
      isInContext: true,
      onLeaveContext: (fn: () => void) => holder.leaveFns.push(fn),
      constRetry,
      child: (name: string) => makeEffects(constRetry, holder.child(name)),
      getHostInfo: async ({ callback }: { callback?: () => void }) => {
        if (callback) hostCallbacks.push({ holder, callback })
        return rows && host(rows)
      },
      action: { createTask },
    }
    holder.leaveFns.push(() => {
      effects.constRetry = undefined
      effects.isInContext = false
    })
    return effects as unknown as Effects
  }
  const effects = makeEffects()
  const primaryUrl = sdk.setupPrimaryUrl({
    id: 'set-primary-url',
    hostId: 'ui-multi',
    interfaceId: 'ui',
    metadata: {
      name: 'Set Primary URL',
      description: '',
      warning: null,
      allowedStatuses: 'any',
      group: null,
      visibility: 'enabled',
    },
    field: { name: 'URL', description: null },
    get: file.read(s => s.primaryUrl),
    set,
  })
  const stored = () =>
    chosen || set.mock.calls.length
      ? JSON.parse(readFileSync(file.path, 'utf-8')).primaryUrl
      : undefined
  const changeRows = (r: T.HostnameInfo[]) => {
    rows = r
    for (const { holder, callback } of hostCallbacks.splice(0))
      if (!holder.left) callback()
  }
  return {
    effects,
    makeEffects,
    createTask,
    set,
    primaryUrl,
    stored,
    changeRows,
  }
}

describe('setupPrimaryUrl', () => {
  describe('action', () => {
    test('offers the non-local addresses, .local with no LAN IP up', async () => {
      const p = setup([bridge, local, onion], 'http://box.local:8080')
      const input = await p.primaryUrl.action.getInput({
        effects: p.effects,
        prefill: null,
      })
      expect(Object.keys((input.spec as any).url.values)).toEqual([
        'http://box.local:8080',
        'http://abc.onion:8080',
      ])
      expect(input.value).toEqual({ url: 'http://box.local:8080' })
    })

    test('pre-fills the stored URL at its hostname’s current port', async () => {
      const p = setup([lan, local], 'http://box.local:9090')
      const input = await p.primaryUrl.action.getInput({
        effects: p.effects,
        prefill: null,
      })
      expect(input.value).toEqual({ url: 'http://box.local:8080' })
    })

    test('pre-fills a stored URL that is gone as it is', async () => {
      const p = setup([lan, local], 'https://app.example.com')
      const input = await p.primaryUrl.action.getInput({
        effects: p.effects,
        prefill: null,
      })
      expect(input.value).toEqual({ url: 'https://app.example.com' })
    })

    test('stores the chosen URL through set', async () => {
      const p = setup([lan, local, onion])
      await p.primaryUrl.action.getInput({ effects: p.effects, prefill: null })
      await p.primaryUrl.action.run({
        effects: p.effects,
        input: { url: 'http://abc.onion:8080' },
      })
      expect(p.stored()).toBe('http://abc.onion:8080')
    })

    test('rejects a URL that is not one of the addresses', async () => {
      const p = setup([lan, local])
      await p.primaryUrl.action.getInput({ effects: p.effects, prefill: null })
      await expect(
        p.primaryUrl.action.run({
          effects: p.effects,
          input: { url: 'http://elsewhere.local:8080' },
        }),
      ).rejects.toThrow()
      expect(p.set).not.toHaveBeenCalled()
    })
  })

  describe('bestUsable', () => {
    test('is the stored URL while it is an address', async () => {
      const p = setup([lan, local, onion], 'http://abc.onion:8080')
      expect(await p.primaryUrl.bestUsable(p.effects).once()).toBe(
        'http://abc.onion:8080',
      )
    })

    test('follows the stored hostname to its current port', async () => {
      const p = setup([lan, local], 'http://box.local:9090')
      expect(await p.primaryUrl.bestUsable(p.effects).once()).toBe(
        'http://box.local:8080',
      )
    })

    test('is the .local address when the stored hostname is gone', async () => {
      const p = setup([lan, local, onion], 'https://app.example.com')
      expect(await p.primaryUrl.bestUsable(p.effects).once()).toBe(
        'http://box.local:8080',
      )
    })

    test('is the .local address when nothing is stored', async () => {
      const p = setup([onion, lan, local])
      expect(await p.primaryUrl.bestUsable(p.effects).once()).toBe(
        'http://box.local:8080',
      )
    })

    test('is the first address when there is no .local one', async () => {
      const p = setup([onion, lan])
      expect(await p.primaryUrl.bestUsable(p.effects).once()).toBe(
        'http://abc.onion:8080',
      )
    })

    test('is the stored URL while the host has no addresses', async () => {
      const p = setup(null, 'https://app.example.com')
      expect(await p.primaryUrl.bestUsable(p.effects).once()).toBe(
        'https://app.example.com',
      )
    })

    test('is null with nothing stored and no addresses', async () => {
      const p = setup([])
      expect(await p.primaryUrl.bestUsable(p.effects).once()).toBeNull()
    })

    test('leaves the store as it is', async () => {
      const p = setup([lan, local], 'https://app.example.com')
      await p.primaryUrl.bestUsable(p.effects).once()
      expect(p.set).not.toHaveBeenCalled()
      expect(p.stored()).toBe('https://app.example.com')
    })

    test('const() re-runs the caller when the addresses change', async () => {
      const p = setup([lan, local], 'http://192.168.1.10:8080')
      const constRetry = jest.fn()
      const effects = p.makeEffects(constRetry)
      expect(await p.primaryUrl.bestUsable(effects).const()).toBe(
        'http://192.168.1.10:8080',
      )
      p.changeRows([local, onion])
      await tick()
      expect(constRetry).toHaveBeenCalledTimes(1)
      expect(await p.primaryUrl.bestUsable(p.effects).once()).toBe(
        'http://box.local:8080',
      )
    })

    test('const() re-runs the caller when the stored URL changes', async () => {
      const p = setup([lan, local, onion], 'http://box.local:8080')
      const constRetry = jest.fn()
      expect(
        await p.primaryUrl.bestUsable(p.makeEffects(constRetry)).const(),
      ).toBe('http://box.local:8080')
      await p.set(p.effects, 'http://abc.onion:8080')
      await tick()
      expect(constRetry).toHaveBeenCalledTimes(1)
    })

    test('const() leaves the caller alone while the URL it resolves to holds', async () => {
      const p = setup([lan, local], 'http://box.local:8080')
      const constRetry = jest.fn()
      await p.primaryUrl.bestUsable(p.makeEffects(constRetry)).const()
      p.changeRows([lan, local, onion])
      await tick()
      expect(constRetry).not.toHaveBeenCalled()
    })

    test('a second read on the same effects keeps the first subscription', async () => {
      const p = setup([lan, local], 'http://192.168.1.10:8080')
      const constRetry = jest.fn()
      const effects = p.makeEffects(constRetry)
      await p.primaryUrl.bestUsable(effects).const()
      await p.primaryUrl.bestUsable(effects).once()
      await p.primaryUrl.bestUsable(effects).const()
      p.changeRows([local, onion])
      await tick()
      expect(constRetry).toHaveBeenCalledTimes(2)
    })

    test('once() leaves the caller alone when the addresses change', async () => {
      const p = setup([lan, local], 'http://192.168.1.10:8080')
      const constRetry = jest.fn()
      await p.primaryUrl.bestUsable(p.makeEffects(constRetry)).once()
      p.changeRows([local, onion])
      await tick()
      expect(constRetry).not.toHaveBeenCalled()
    })
  })

  describe('setupTask', () => {
    const run = (
      p: ReturnType<typeof setup>,
      ...args: Parameters<ReturnType<typeof setup>['primaryUrl']['setupTask']>
    ) => p.primaryUrl.setupTask(...args).init(p.effects, null)

    test('declares the addresses the stored URL must be one of', async () => {
      const p = setup([lan, local, onion], 'http://box.local:8080')
      await run(p, 'important', { reason: 'Choose a URL' })
      expect(p.createTask).toHaveBeenCalledWith({
        actionId: 'set-primary-url',
        packageId: 'testOutput',
        replayId: 'testOutput:set-primary-url',
        severity: 'important',
        reason: 'Choose a URL',
        when: { condition: 'input-not-matches', once: false },
        input: {
          kind: 'partial',
          accept: [
            { url: 'http://192.168.1.10:8080' },
            { url: 'http://box.local:8080' },
            { url: 'http://abc.onion:8080' },
          ],
          set: { url: 'http://box.local:8080' },
        },
      })
      expect(p.set).not.toHaveBeenCalled()
    })

    test('passes the severity and replay id through', async () => {
      const p = setup([lan, local])
      await run(p, 'critical', { replayId: 'primary-url' })
      expect(p.createTask).toHaveBeenCalledWith(
        expect.objectContaining({
          severity: 'critical',
          replayId: 'primary-url',
        }),
      )
    })

    test('raises nothing while the interface has no addresses', async () => {
      const p = setup(null, 'http://box.local:8080')
      await run(p, 'important')
      expect(p.createTask).not.toHaveBeenCalled()
    })

    test('re-runs when the addresses change', async () => {
      const p = setup([lan, local], 'http://box.local:8080')
      const constRetry = jest.fn()
      await p.primaryUrl
        .setupTask('important')
        .init(p.makeEffects(constRetry), null)
      p.changeRows([local, onion])
      await tick()
      expect(constRetry).toHaveBeenCalledTimes(1)
    })
  })
})
