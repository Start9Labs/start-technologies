import { getSharedVolumes, Watchable } from '@start9labs/start-core/util'
import * as T from '@start9labs/start-core/types'
import { Daemons, configHash } from '../mainFn/Daemons'
import { Mounts } from '../mainFn/Mounts'
import { SubContainer } from '../util/SubContainer'

const mockLifecycle: string[] = []
let mockStartGate: Promise<void> | undefined
jest.mock('../mainFn/HealthDaemon', () => ({
  ...jest.requireActual('../mainFn/HealthDaemon'),
  HealthDaemon: jest
    .fn()
    .mockImplementation((_daemon, _dependencies, id: string) => ({
      updateStatus: async () => {
        mockLifecycle.push(`start:${id}`)
        await mockStartGate
      },
      term: async () => {
        mockLifecycle.push(`stop:${id}`)
      },
    })),
}))

type Manifest = T.SDKManifest & { sharedVolumes: true }
const id = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
const ready = {
  display: 'Ready',
  fn: () => ({ result: 'success' as const, message: null }),
}
const settle = () => new Promise<void>(resolve => setImmediate(resolve))

function runtime() {
  type Scope = {
    active: boolean
    leave: (() => void | null | undefined)[]
    children: Map<string, Scope>
  }
  const scope = (): Scope => ({ active: true, leave: [], children: new Map() })
  const close = (node: Scope) => {
    node.active = false
    for (const child of node.children.values()) close(child)
    for (const fn of node.leave) fn()
  }
  const root = scope()
  const callbacks: { node: Scope; callback: () => void }[] = []
  const restart = jest.fn()
  const catalog: T.GrantedSharedVolume[] = [
    { id, name: 'Media', readonly: true, availability: 'available' },
  ]
  const effectsFor = (node: Scope): T.Effects =>
    ({
      child: name => {
        const previous = node.children.get(name)
        if (previous) close(previous)
        const next = scope()
        node.children.set(name, next)
        return effectsFor(next)
      },
      get isInContext() {
        return node.active
      },
      onLeaveContext: fn => {
        node.leave.push(fn)
      },
      constRetry: restart,
      getSharedVolumes: async ({ callback }) => {
        if (callback) callbacks.push({ node, callback })
        return catalog.map(volume => ({ ...volume }))
      },
    }) satisfies Partial<T.Effects> as unknown as T.Effects
  return {
    effects: effectsFor(root),
    restart,
    sourceChanged: async () => {
      for (const { node, callback } of callbacks.splice(0))
        if (node.active) callback()
      await settle()
    },
    close: () => close(root),
  }
}

function container(effects: T.Effects, name: string, shared = false) {
  const mounts = shared
    ? Mounts.of<Manifest>().mountSharedVolume({
        id,
        mountpoint: '/media',
        readonly: true,
      })
    : null
  return SubContainer.of<Manifest>(effects, { imageId: 'main' }, mounts, name)
}

function topology(effects: T.Effects, command = 'consume') {
  const shared = container(effects, 'shared', true)
  return Daemons.of<Manifest>({ effects })
    .addDaemon('consumer', {
      subcontainer: shared,
      exec: { command: [command] },
      ready,
      requires: [],
    })
    .addDaemon('peer', {
      subcontainer: shared,
      exec: { command: ['peer'] },
      ready,
      requires: [],
    })
    .addDaemon('dependent', {
      subcontainer: container(effects, 'dependent'),
      exec: { command: ['dependent'] },
      ready,
      requires: ['consumer'],
    })
    .addDaemon('unrelated', {
      subcontainer: container(effects, 'unrelated'),
      exec: { command: ['unrelated'] },
      ready,
      requires: [],
    })
}

beforeEach(() => {
  mockLifecycle.length = 0
  mockStartGate = undefined
})

test('equal-metadata source invalidation remounts consumers, shared peers and dependents, not unrelated entries', async () => {
  const r = runtime()
  const hashes: string[][] = []
  const reconciler = Daemons.dynamic<Manifest>(
    r.effects,
    async ({ effects }) => {
      await getSharedVolumes(effects.child('catalog')).const()
      const recorded = topology(effects)
      hashes.push(recorded.entries.map(configHash))
      return recorded
    },
  )
  const built = await reconciler.build()
  try {
    mockLifecycle.length = 0
    reconciler.scheduleRerun()
    await settle()
    expect(mockLifecycle).toEqual([])
    await r.sourceChanged()
    expect(hashes[0]).toEqual(hashes[2])
    expect(
      mockLifecycle.filter(event => event.startsWith('stop:')).sort(),
    ).toEqual(['stop:consumer', 'stop:dependent', 'stop:peer'])
    expect(
      mockLifecycle.filter(event => event.startsWith('start:')).sort(),
    ).toEqual(['start:consumer', 'start:dependent', 'start:peer'])
    expect(mockLifecycle.indexOf('stop:dependent')).toBeLessThan(
      mockLifecycle.indexOf('stop:consumer'),
    )
    expect(r.restart).not.toHaveBeenCalled()
  } finally {
    await built.term()
    r.close()
  }
})

test('a composed reader cannot hide source invalidation behind equal mapped metadata', async () => {
  const r = runtime()
  const reconciler = Daemons.dynamic<Manifest>(
    r.effects,
    async ({ effects }) => {
      await Watchable.combine(
        effects,
        [getSharedVolumes(effects)],
        ([volumes]) => volumes.map(volume => volume.id),
      ).const()
      return topology(effects)
    },
  )
  const built = await reconciler.build()
  try {
    mockLifecycle.length = 0
    await r.sourceChanged()
    expect(mockLifecycle).toContain('stop:consumer')
    expect(mockLifecycle).toContain('start:consumer')
    expect(mockLifecycle).not.toContain('stop:unrelated')
  } finally {
    await built.term()
    r.close()
  }
})

test('source invalidation during a restart remains pending for the next reconcile', async () => {
  const r = runtime()
  const reconciler = Daemons.dynamic<Manifest>(
    r.effects,
    async ({ effects }) => {
      await getSharedVolumes(effects).const()
      return topology(effects)
    },
  )
  const built = await reconciler.build()
  let release!: () => void
  try {
    mockLifecycle.length = 0
    mockStartGate = new Promise<void>(resolve => {
      release = resolve
    })
    await r.sourceChanged()
    expect(mockLifecycle).toContain('start:consumer')
    await r.sourceChanged()
    mockStartGate = undefined
    release()
    await settle()
    expect(
      mockLifecycle.filter(event => event === 'start:consumer'),
    ).toHaveLength(2)
    expect(
      mockLifecycle.filter(event => event === 'stop:consumer'),
    ).toHaveLength(2)
    expect(mockLifecycle).not.toContain('stop:unrelated')
  } finally {
    release?.()
    mockStartGate = undefined
    await built.term()
    r.close()
  }
})

test('a failed rebuild preserves invalidation for a later unchanged successful builder', async () => {
  const r = runtime()
  let fail = false
  const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
  const reconciler = Daemons.dynamic<Manifest>(
    r.effects,
    async ({ effects }) => {
      await getSharedVolumes(effects).const()
      if (fail) throw new Error('temporary configuration error')
      return topology(effects)
    },
  )
  const built = await reconciler.build()
  try {
    mockLifecycle.length = 0
    fail = true
    await r.sourceChanged()
    expect(mockLifecycle).toEqual([])
    fail = false
    reconciler.scheduleRerun()
    await settle()
    expect(mockLifecycle).toContain('stop:consumer')
    expect(mockLifecycle).toContain('start:consumer')
    expect(mockLifecycle).not.toContain('stop:unrelated')
  } finally {
    await built.term()
    r.close()
    logged.mockRestore()
  }
})

test('changing one command restarts every process sharing its container', async () => {
  const r = runtime()
  let command = 'one'
  const reconciler = Daemons.dynamic<Manifest>(r.effects, ({ effects }) =>
    topology(effects, command),
  )
  const built = await reconciler.build()
  try {
    mockLifecycle.length = 0
    command = 'two'
    reconciler.scheduleRerun()
    await settle()
    expect(mockLifecycle).toContain('stop:peer')
    expect(mockLifecycle).toContain('start:peer')
    expect(mockLifecycle).not.toContain('stop:unrelated')
  } finally {
    await built.term()
    r.close()
  }
})

test('sharing-topology changes reconcile even when individual structural hashes match', async () => {
  const r = runtime()
  let share = false
  const hashes: string[][] = []
  const reconciler = Daemons.dynamic<Manifest>(r.effects, ({ effects }) => {
    const left = container(effects, 'same')
    const right = share ? left : container(effects, 'same')
    const recorded = Daemons.of<Manifest>({ effects })
      .addDaemon('left', {
        subcontainer: left,
        exec: { command: ['left'] },
        ready,
        requires: [],
      })
      .addDaemon('right', {
        subcontainer: right,
        exec: { command: ['right'] },
        ready,
        requires: [],
      })
    hashes.push(recorded.entries.map(configHash))
    return recorded
  })
  const built = await reconciler.build()
  try {
    mockLifecycle.length = 0
    share = true
    reconciler.scheduleRerun()
    await settle()
    expect(hashes[0]).toEqual(hashes[1])
    expect(
      mockLifecycle.filter(event => event.startsWith('stop:')).sort(),
    ).toEqual(['stop:left', 'stop:right'])
    expect(
      mockLifecycle.filter(event => event.startsWith('start:')).sort(),
    ).toEqual(['start:left', 'start:right'])
  } finally {
    await built.term()
    r.close()
  }
})
