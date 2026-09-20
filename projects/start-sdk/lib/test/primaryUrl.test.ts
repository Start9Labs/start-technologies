import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { Effects } from '@start9labs/start-core/Effects'
import * as T from '@start9labs/start-core/types'
import { z } from '@start9labs/start-core/zExport'
import { FileHelper } from '../util/fileHelper'
import { sdk } from './output.sdk'

const dir = mkdtempSync(join(tmpdir(), 'primary-url-'))
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
const domain = row(
  'app.example.com',
  { kind: 'public-domain', gateway: 'eth0' },
  443,
  true,
)
const bridge = row('10.0.3.1', { kind: 'ipv4', gateway: 'lxcbr0' })
const wifi = row('192.168.2.10', { kind: 'ipv4', gateway: 'wlan0' })
const renamed = row('newbox.local', { kind: 'mdns', gateways: ['eth0'] })

const host = (available: T.HostnameInfo[]): T.Host => ({
  bindings: {
    80: {
      enabled: true,
      options: { preferredExternalPort: 80, addSsl: null, secure: null },
      net: { assignedPort: 8080, assignedSslPort: 443 },
      addresses: { enabled: [], disabled: [], guaWan: [], available },
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

const setup = (
  available: T.HostnameInfo[],
  chosen?: string,
  params: {
    severity?: T.TaskSeverity
    actionId?: string
    onRemoved?: 'fallback' | 'task'
  } = {},
  storeName = 'store.json',
) => {
  const sub = mkdtempSync(join(dir, 'case-'))
  const file = FileHelper.json(join(sub, storeName), shape)
  if (chosen) writeFileSync(file.path, JSON.stringify({ primaryUrl: chosen }))
  let rows = available
  const createTask = jest.fn(async () => null)
  const clearTasks = jest.fn(async () => null)
  const effects = {
    eventId: 'event',
    isInContext: true,
    onLeaveContext: () => {},
    child: () => effects,
    getHostInfo: async () => host(rows),
    action: { createTask, clearTasks },
  } as unknown as Effects
  const primaryUrl = sdk.setupPrimaryUrl({
    hostId: 'ui-multi',
    interfaceId: 'ui',
    store: {
      file,
      get: s => s.primaryUrl,
      set: url => ({ primaryUrl: url }),
    },
    ...params,
  })
  const run = () => primaryUrl.init.init(effects, null)
  const stored = () => JSON.parse(readFileSync(file.path, 'utf-8')).primaryUrl
  const record = `${file.path}.${params.actionId ?? 'set-primary-url'}.json`
  const source = () => JSON.parse(readFileSync(record, 'utf-8'))
  const setRows = (r: T.HostnameInfo[]) => {
    rows = r
  }
  return {
    effects,
    createTask,
    clearTasks,
    primaryUrl,
    run,
    stored,
    record,
    source,
    setRows,
  }
}

describe('setupPrimaryUrl', () => {
  test('stores the .local address when nothing is chosen', async () => {
    const p = setup([onion, lan, local])
    await p.run()
    expect(p.stored()).toBe('http://box.local:8080')
    expect(p.createTask).not.toHaveBeenCalled()
  })

  test('stores the first address when there is no .local one', async () => {
    const p = setup([onion, domain])
    await p.run()
    expect(p.stored()).toBe('http://abc.onion:8080')
  })

  test('keeps a listed choice and clears the task', async () => {
    const p = setup([lan, local], 'http://box.local:8080')
    await p.run()
    expect(p.clearTasks).toHaveBeenCalledWith({
      only: ['testOutput:set-primary-url'],
    })
    expect(p.createTask).not.toHaveBeenCalled()
  })

  test('keeps a .local choice while no LAN IP resolves it', async () => {
    const p = setup([local, onion], 'http://box.local:8080')
    await p.run()
    expect(p.createTask).not.toHaveBeenCalled()
    expect(p.clearTasks).toHaveBeenCalled()
  })

  test('keeps a .local choice while the LAN is down, bridge up or not', async () => {
    const p = setup([bridge, onion], 'http://box.local:8080')
    await p.run()
    expect(p.createTask).not.toHaveBeenCalled()
    expect(p.clearTasks).not.toHaveBeenCalled()
    expect(p.stored()).toBe('http://box.local:8080')
  })

  test('keeps a LAN IP choice while the LAN is down', async () => {
    const p = setup([bridge, onion], 'http://192.168.1.10:8080')
    await p.run()
    expect(p.createTask).not.toHaveBeenCalled()
  })

  test('raises the task for a domain that is gone while the LAN is down', async () => {
    const p = setup([bridge, onion], 'https://app.example.com', {
      onRemoved: 'task',
    })
    await p.run()
    expect(p.createTask).toHaveBeenCalled()
  })

  test('raises the task when the server was renamed', async () => {
    const p = setup([lan, renamed], 'http://box.local:8080', {
      onRemoved: 'task',
    })
    await p.run()
    expect(p.createTask).toHaveBeenCalled()
  })

  test('falls back to the .local address when the chosen domain is gone', async () => {
    const p = setup([lan, local], 'https://app.example.com')
    await p.run()
    expect(p.stored()).toBe('http://box.local:8080')
    expect(p.createTask).not.toHaveBeenCalled()
    expect(p.clearTasks).toHaveBeenCalled()
  })

  test('follows a renamed server to its new .local address', async () => {
    const p = setup([lan, renamed], 'http://box.local:8080')
    await p.run()
    expect(p.stored()).toBe('http://newbox.local:8080')
    expect(p.createTask).not.toHaveBeenCalled()
  })

  test('raises the task when there is nothing to fall back to', async () => {
    const p = setup([], 'https://app.example.com')
    await p.run()
    expect(p.createTask).toHaveBeenCalled()
  })

  test('keeps an IP choice while its own interface is down and another is up', async () => {
    const p = setup([lan, local], 'http://192.168.1.10:8080')
    await p.run()
    expect(p.source().gateway).toBe('eth0')
    p.setRows([bridge, wifi, onion])
    await p.run()
    expect(p.createTask).not.toHaveBeenCalled()
  })

  test('stays quiet about an IP it never saw on an interface', async () => {
    const p = setup([bridge, wifi, onion], 'http://192.168.1.10:8080')
    await p.run()
    expect(p.createTask).not.toHaveBeenCalled()
  })

  test('raises the task when the interface comes back with another IP', async () => {
    const p = setup([lan, local], 'http://192.168.1.10:8080', {
      onRemoved: 'task',
    })
    await p.run()
    p.setRows([
      bridge,
      row('192.168.1.20', { kind: 'ipv4', gateway: 'eth0' }),
      local,
    ])
    await p.run()
    expect(p.createTask).toHaveBeenCalled()
  })

  test('never claims a store named like its own record', async () => {
    const p = setup([lan, local], undefined, {}, 'primary-url.json')
    await p.run()
    expect(p.stored()).toBe('http://box.local:8080')
    expect(p.source()).toEqual({ url: 'http://box.local:8080', gateway: null })
  })

  test('rewrites a damaged record instead of failing', async () => {
    const p = setup([lan, local], 'http://192.168.1.10:8080')
    writeFileSync(p.record, '{not json')
    await p.run()
    expect(p.source().gateway).toBe('eth0')
  })

  test('keeps one record per helper instance', async () => {
    const p = setup([lan, local], 'http://192.168.1.10:8080', {
      actionId: 'set-api-url',
    })
    await p.run()
    expect(p.source().gateway).toBe('eth0')
    expect(existsSync(p.record.replace('set-api-url', 'set-primary-url'))).toBe(
      false,
    )
  })

  test('follows a port change of the chosen hostname', async () => {
    const p = setup([lan, local], 'http://box.local:9090')
    await p.run()
    expect(p.stored()).toBe('http://box.local:8080')
    expect(p.clearTasks).toHaveBeenCalled()
    expect(p.createTask).not.toHaveBeenCalled()
  })

  test('raises an important task when the chosen hostname is gone', async () => {
    const p = setup([lan, local], 'https://app.example.com', {
      onRemoved: 'task',
    })
    await p.run()
    expect(p.createTask).toHaveBeenCalledWith(
      expect.objectContaining({
        packageId: 'testOutput',
        actionId: 'set-primary-url',
        severity: 'important',
        replayId: 'testOutput:set-primary-url',
      }),
    )
    expect(p.stored()).toBe('https://app.example.com')
  })

  test('raises the task at the given severity', async () => {
    const p = setup([lan, local], 'https://app.example.com', {
      severity: 'critical',
      onRemoved: 'task',
    })
    await p.run()
    expect(p.createTask).toHaveBeenCalledWith(
      expect.objectContaining({ severity: 'critical' }),
    )
  })

  test('the action offers every configured address and prefills the choice', async () => {
    const p = setup([local, onion], 'http://box.local:8080')
    const input = await p.primaryUrl.action.getInput({
      effects: p.effects,
      prefill: null,
    })
    expect(input.value).toEqual({ url: 'http://box.local:8080' })
    expect(Object.keys((input.spec as any).url.values)).toEqual([
      'http://box.local:8080',
      'http://abc.onion:8080',
    ])

    await p.primaryUrl.action.run({
      effects: p.effects,
      input: { url: 'http://abc.onion:8080' },
    })
    expect(p.stored()).toBe('http://abc.onion:8080')
    expect(p.source()).toEqual({ url: 'http://abc.onion:8080', gateway: null })
  })
})
