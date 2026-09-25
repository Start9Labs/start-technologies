import type { Effects } from '@start9labs/start-core/types'
import { Watchable } from '@start9labs/start-core/util/Watchable'
import { Dependencies, Dependency } from '../dependencies'

function mockEffects() {
  const children = new Map<string, Effects>()
  const setDependencies = jest.fn(
    async (_: Parameters<Effects['setDependencies']>[0]) => null,
  )
  const clearTasks = jest.fn(
    async (_: Parameters<Effects['action']['clearTasks']>[0]) => null,
  )
  const effects = {
    setDependencies,
    action: { clearTasks },
    child: jest.fn((name: string) => {
      const child = { ...effects, constRetry: undefined } as Effects
      children.set(name, child)
      return child
    }),
  } as unknown as Effects
  return { effects, children, setDependencies, clearTasks }
}

test('the published base is also the runtime requirement', async () => {
  const { effects, setDependencies } = mockEffects()
  const dependencies = Dependencies.of().addDependency(
    Dependency.required('bitcoind', {
      description: 'Blockchain data',
      metadata: { title: 'Bitcoin', icon: 'https://example.com/icon.png' },
      versionRange: '>=28.4:17',
      kind: 'running',
      healthChecks: ['bitcoin-rest'],
    }),
  )
  expect(dependencies.manifestDependencies().bitcoind.versionRange).toBe(
    '>=28.4:17',
  )
  await dependencies.init(effects)
  expect(setDependencies).toHaveBeenCalledWith({
    dependencies: [
      {
        id: 'bitcoind',
        kind: 'running',
        versionRange: '>=28.4:17',
        healthChecks: ['bitcoin-rest'],
      },
    ],
  })
})

test('optional dependencies react to enablement and clear their tasks', async () => {
  let enabled = true
  const { effects, setDependencies, clearTasks } = mockEffects()
  const init = jest.fn(async () => {})
  const dependencies = Dependencies.of().addDependency(
    Dependency.optional('lnd', {
      description: 'Lightning',
      metadata: { title: 'LND', icon: 'https://example.com/icon.png' },
      versionRange: '>=0.20:0',
      kind: 'exists',
      enabled: async () => enabled,
    })
      .withDynamicNarrowing(async () => ({
        kind: 'running',
        versionRange: '>=0.21:0',
        healthChecks: ['lnd'],
      }))
      .withInit(init, ['lnd:autoconfig']),
  )
  expect(dependencies.manifestDependencies().lnd).toMatchObject({
    optional: true,
    kind: 'exists',
    versionRange: '>=0.20:0',
  })
  await dependencies.init(effects)
  expect(setDependencies.mock.calls[0][0].dependencies[0]).toMatchObject({
    kind: 'running',
    healthChecks: ['lnd'],
  })
  expect(init).toHaveBeenCalledTimes(1)
  enabled = false
  await dependencies.init(effects)
  expect(clearTasks).toHaveBeenCalledWith({ only: ['lnd:autoconfig'] })
  expect(setDependencies).toHaveBeenLastCalledWith({ dependencies: [] })
  expect(init).toHaveBeenCalledTimes(1)
})

test('a dependency watcher reruns only its own child scope', async () => {
  const { effects, children, setDependencies, clearTasks } = mockEffects()
  const parentRetry = jest.fn()
  effects.constRetry = parentRetry
  let enabled = true
  const optionalInit = jest.fn(async () => {})
  const requiredInit = jest.fn(async () => {})
  const dependencies = Dependencies.of()
    .addDependency(
      Dependency.required('bitcoind', {
        description: null,
        metadata: { title: 'Bitcoin', icon: 'https://example.com/bitcoin.png' },
        versionRange: '*',
        kind: 'exists',
      }).withInit(requiredInit),
    )
    .addDependency(
      Dependency.optional('lnd', {
        description: null,
        metadata: { title: 'LND', icon: 'https://example.com/lnd.png' },
        versionRange: '*',
        kind: 'exists',
        enabled: async () => enabled,
      }).withInit(optionalInit, ['lnd:setup']),
    )
  await dependencies.init(effects, 'install')
  expect(setDependencies).toHaveBeenCalledTimes(1)
  expect(requiredInit).toHaveBeenCalledTimes(1)
  expect(optionalInit).toHaveBeenCalledWith(
    children.get('dependency_lnd_init_0'),
    'install',
    undefined,
  )

  enabled = false
  const oldChild = children.get('dependency_lnd')!
  await (oldChild.constRetry!() as unknown as Promise<void>)
  await (children.get('dependency_lnd_init_0')!
    .constRetry!() as unknown as Promise<void>)
  expect(effects.child).toHaveBeenCalledTimes(6)
  expect(children.get('dependency_lnd')).not.toBe(oldChild)
  expect(clearTasks).toHaveBeenCalledWith({ only: ['lnd:setup'] })
  expect(requiredInit).toHaveBeenCalledTimes(1)
  expect(optionalInit).toHaveBeenCalledTimes(1)
  expect(parentRetry).not.toHaveBeenCalled()
  expect(setDependencies).toHaveBeenLastCalledWith({
    dependencies: [{ id: 'bitcoind', kind: 'exists', versionRange: '*' }],
  })

  enabled = true
  await (children.get('dependency_lnd')!
    .constRetry!() as unknown as Promise<void>)
  await (children.get('dependency_lnd_init_0')!
    .constRetry!() as unknown as Promise<void>)
  expect(optionalInit).toHaveBeenCalledTimes(2)
  expect(optionalInit).toHaveBeenLastCalledWith(
    children.get('dependency_lnd_init_0'),
    null,
    expect.anything(),
  )
  expect(requiredInit).toHaveBeenCalledTimes(1)
  expect(setDependencies).toHaveBeenCalledTimes(3)

  await (children.get('dependency_bitcoind_init_0')!
    .constRetry!() as unknown as Promise<void>)
  expect(requiredInit).toHaveBeenCalledTimes(2)
  expect(requiredInit).toHaveBeenLastCalledWith(
    children.get('dependency_bitcoind_init_0'),
    null,
    expect.anything(),
  )
  expect(optionalInit).toHaveBeenCalledTimes(2)
  expect(setDependencies).toHaveBeenCalledTimes(3)
  expect(parentRetry).not.toHaveBeenCalled()
})

test('multiple inits of one dependency react independently', async () => {
  const { effects, children, setDependencies, clearTasks } = mockEffects()
  let enabled = true
  const firstInit = jest.fn(async () => {})
  const secondInit = jest.fn(async () => {})
  const narrowing = jest.fn(async () => null)
  const dependencies = Dependencies.of().addDependency(
    Dependency.optional('lnd', {
      description: null,
      metadata: { title: 'LND', icon: 'https://example.com/icon.png' },
      versionRange: '*',
      kind: 'exists',
      enabled: async () => enabled,
    })
      .withDynamicNarrowing(narrowing)
      .withInit(firstInit, ['lnd:first'])
      .withInit({ init: secondInit }, ['lnd:second']),
  )
  await dependencies.init(effects, 'update')
  expect(firstInit).toHaveBeenCalledWith(
    children.get('dependency_lnd_init_0'),
    'update',
    undefined,
  )
  expect(secondInit).toHaveBeenCalledWith(
    children.get('dependency_lnd_init_1'),
    'update',
    undefined,
  )
  expect(narrowing).toHaveBeenCalledTimes(1)
  expect(setDependencies).toHaveBeenCalledTimes(1)

  await (children.get('dependency_lnd_init_0')!
    .constRetry!() as unknown as Promise<void>)
  expect(firstInit).toHaveBeenCalledTimes(2)
  expect(firstInit).toHaveBeenLastCalledWith(
    children.get('dependency_lnd_init_0'),
    null,
    expect.anything(),
  )
  expect(secondInit).toHaveBeenCalledTimes(1)
  expect(narrowing).toHaveBeenCalledTimes(1)
  expect(setDependencies).toHaveBeenCalledTimes(1)

  await (children.get('dependency_lnd_init_1')!
    .constRetry!() as unknown as Promise<void>)
  expect(secondInit).toHaveBeenCalledTimes(2)
  expect(firstInit).toHaveBeenCalledTimes(2)
  expect(setDependencies).toHaveBeenCalledTimes(1)

  enabled = false
  for (const name of [
    'dependency_lnd',
    'dependency_lnd_init_0',
    'dependency_lnd_init_1',
  ]) {
    await (children.get(name)!.constRetry!() as unknown as Promise<void>)
  }
  expect(setDependencies).toHaveBeenLastCalledWith({ dependencies: [] })
  expect(clearTasks.mock.calls.map(([arg]) => arg)).toEqual([
    { only: ['lnd:first'] },
    { only: ['lnd:second'] },
  ])
  expect(firstInit).toHaveBeenCalledTimes(2)
  expect(secondInit).toHaveBeenCalledTimes(2)

  enabled = true
  for (const name of [
    'dependency_lnd',
    'dependency_lnd_init_0',
    'dependency_lnd_init_1',
  ]) {
    await (children.get(name)!.constRetry!() as unknown as Promise<void>)
  }
  expect(setDependencies).toHaveBeenCalledTimes(3)
  expect(firstInit).toHaveBeenCalledTimes(3)
  expect(secondInit).toHaveBeenCalledTimes(3)
  expect(narrowing).toHaveBeenCalledTimes(2)
})

test('a watched value inside one init only reruns that init', async () => {
  const { effects, children, setDependencies } = mockEffects()
  let value = 0
  let signal: (() => void) | undefined
  const source = {
    once: async () => value,
    async *watch(abort?: AbortSignal) {
      yield value
      while (!abort?.aborted) {
        await new Promise<void>(resolve => {
          signal = resolve
          abort?.addEventListener('abort', () => resolve(), { once: true })
        })
        if (abort?.aborted) return
        yield value
      }
    },
  }
  const firstInit = jest.fn(async (child: Effects) => {
    await Watchable.from(child, source).const()
  })
  const secondInit = jest.fn(async () => {})
  const dependencies = Dependencies.of().addDependency(
    Dependency.required('bitcoind', {
      description: null,
      metadata: { title: 'Bitcoin', icon: 'https://example.com/icon.png' },
      versionRange: '*',
      kind: 'exists',
    })
      .withInit(firstInit)
      .withInit(secondInit),
  )
  await dependencies.init(effects)
  const oldChild = children.get('dependency_bitcoind_init_0')
  await new Promise(resolve => setImmediate(resolve))
  value = 1
  signal!()
  await new Promise(resolve => setImmediate(resolve))

  expect(firstInit).toHaveBeenCalledTimes(2)
  expect(children.get('dependency_bitcoind_init_0')).not.toBe(oldChild)
  expect(secondInit).toHaveBeenCalledTimes(1)
  expect(setDependencies).toHaveBeenCalledTimes(1)
})

test('overlapping dependency changes publish ordered snapshots', async () => {
  const { effects, children, setDependencies } = mockEffects()
  let enabledA = true
  let enabledB = true
  let release!: () => void
  const blocked = new Promise<void>(resolve => (release = resolve))
  const snapshots: string[][] = []
  setDependencies.mockImplementation(async ({ dependencies }) => {
    snapshots.push(dependencies.map(dep => dep.id))
    if (snapshots.length === 2) await blocked
    return null
  })
  const base = {
    description: null,
    metadata: { title: 'Dependency', icon: 'https://example.com/icon.png' },
    versionRange: '*',
    kind: 'exists' as const,
  }
  const dependencies = Dependencies.of()
    .addDependency(
      Dependency.optional('a', { ...base, enabled: async () => enabledA }),
    )
    .addDependency(
      Dependency.optional('b', { ...base, enabled: async () => enabledB }),
    )
  await dependencies.init(effects)

  enabledA = false
  const first = children.get('dependency_a')!
    .constRetry!() as unknown as Promise<void>
  await new Promise(resolve => setImmediate(resolve))
  enabledB = false
  const second = children.get('dependency_b')!
    .constRetry!() as unknown as Promise<void>
  await new Promise(resolve => setImmediate(resolve))
  expect(snapshots).toEqual([['a', 'b'], ['b']])
  release()
  await Promise.all([first, second])
  expect(snapshots).toEqual([['a', 'b'], ['b'], []])
})

test('init scripts receive the lifecycle kind and require task IDs when optional', async () => {
  const base = {
    description: null,
    metadata: { title: 'Bitcoin', icon: 'https://example.com/icon.png' },
    versionRange: '*',
    kind: 'exists' as const,
  }
  const init = jest.fn(async () => {})
  const dependency = Dependency.required('bitcoind', base).withInit({ init })
  const { effects, children } = mockEffects()
  await Dependencies.of().addDependency(dependency).init(effects, 'restore')
  expect(init).toHaveBeenCalledWith(
    children.get('dependency_bitcoind_init_0'),
    'restore',
    undefined,
  )
  expect(() =>
    Dependency.optional('lnd', { ...base, enabled: async () => true }).withInit(
      async () => {},
    ),
  ).toThrow('task replay IDs')
})

test('disjoint runtime narrowing is rejected', async () => {
  const dependencies = Dependencies.of().addDependency(
    Dependency.required('bitcoind', {
      description: null,
      metadata: { title: 'Bitcoin', icon: 'https://example.com/icon.png' },
      versionRange: '<2:0',
      kind: 'exists',
    }).withDynamicNarrowing(async () => ({ versionRange: '>=3:0' })),
  )
  await expect(dependencies.init(mockEffects().effects)).rejects.toThrow(
    'incompatible version range',
  )
})
