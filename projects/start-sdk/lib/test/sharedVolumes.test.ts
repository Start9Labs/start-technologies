import * as cp from 'child_process'
import { EventEmitter } from 'events'
import * as fs from 'fs/promises'
import * as T from '@start9labs/start-core/types'
import { getSharedVolumes } from '@start9labs/start-core/util'
import { StartSdk } from '../StartSdk'
import { Mounts } from '../mainFn/Mounts'
import { onceTrigger } from '../trigger'
import { configHash, Daemons } from '../mainFn/Daemons'
import { setupManifest } from '../manifest/setupManifest'
import { SubContainer } from '../util/SubContainer'
import * as subcontainers from '../util/SubContainer'

jest.mock('fs/promises', () => ({
  ...jest.requireActual('fs/promises'),
  stat: jest.fn(),
  mkdir: jest.fn(),
  copyFile: jest.fn(),
  rm: jest.fn(),
}))

jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  spawn: jest.fn(),
}))

type Manifest = T.SDKManifest & { sharedVolumes: true }
const id = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
const options = {
  id,
  mountpoint: '/media/music',
  subpath: 'albums',
  readonly: true,
}
const essential = () => Mounts.of<Manifest>().mountSharedVolume(options)

const stagedPath = '/staged-source'
const placeholderPath = '/staged-unavailable'
function prepared(result: T.SharedVolumeMountResult): T.PreparedSharedVolume {
  return { path: stagedPath, placeholder: placeholderPath, result }
}

function context() {
  return {
    onLeaveContext: jest.fn(),
    isInContext: true,
    constRetry: jest.fn(),
    subcontainer: {
      createFs: jest.fn(async () => ['/rootfs', 'guid']),
      destroyFs: jest.fn(async () => null),
    },
    mount: jest.fn(async () => null),
    prepareSharedVolume: jest.fn(
      async (): Promise<T.PreparedSharedVolume> =>
        prepared({ kind: 'mounted' }),
    ),
    getSharedVolumes: jest.fn(),
  } as unknown as T.Effects
}

beforeEach(() => {
  jest.mocked(fs.stat).mockRejectedValue(new Error('ENOENT'))
  jest.mocked(fs.mkdir).mockResolvedValue(undefined)
  jest.mocked(fs.copyFile).mockResolvedValue(undefined)
  jest.mocked(fs.rm).mockResolvedValue(undefined)
  jest
    .spyOn(subcontainers, 'execFile')
    .mockResolvedValue({ stdout: '', stderr: '' })
  jest.mocked(cp.spawn).mockImplementation(() => {
    const leader = new EventEmitter()
    return Object.assign(leader, {
      kill: jest.fn(() => {
        queueMicrotask(() => leader.emit('exit', 0))
        return true
      }),
    }) as unknown as cp.ChildProcessWithoutNullStreams
  })
})

afterEach(() => jest.restoreAllMocks())

test('shared mounts are immutable, preserve IDs and access mode, and survive other builder calls', () => {
  const empty = Mounts.of<Manifest>()
  const shared = empty.mountSharedVolume(options)
  const built = shared
    .mountVolume({
      volumeId: 'main',
      mountpoint: '/data',
      subpath: null,
      readonly: false,
    })
    .mountAssets({ mountpoint: '/assets', subpath: null })
    .mountDependency({
      dependencyId: 'peer',
      volumeId: 'main',
      mountpoint: '/peer',
      subpath: null,
      readonly: true,
    })
    .build()
  expect(empty.build()).toEqual([])
  expect(shared.build()).toEqual([
    {
      mountpoint: options.mountpoint,
      options: { type: 'shared', id, subpath: 'albums', readonly: true },
    },
  ])
  expect(built.map(m => m.options.type)).toEqual([
    'volume',
    'assets',
    'shared',
    'pointer',
  ])
  expect(built.find(m => m.options.type === 'shared')).toEqual(
    shared.build()[0],
  )
})

test('shared-volume declarations participate in daemon config hashes without materialization', () => {
  const effects = context()
  const hash = (mountOptions: typeof options) => {
    const subcontainer = SubContainer.of<Manifest>(
      effects,
      { imageId: 'main' },
      Mounts.of<Manifest>().mountSharedVolume(mountOptions),
      'music',
    )
    const builder = Daemons.of<Manifest>({ effects }).addDaemon('music', {
      subcontainer,
      exec: { command: ['music-server'] },
      ready: {
        display: 'Music',
        fn: async () => ({ result: 'success', message: null }),
      },
      requires: [],
    })
    return configHash(builder.entries[0])
  }
  expect(hash(options)).toBe(hash({ ...options }))
  for (const changed of [
    { readonly: false },
    { subpath: 'other' },
    { id: 'another' },
  ]) {
    expect(hash({ ...options, ...changed })).not.toBe(hash(options))
  }
  expect(effects.subcontainer.createFs).not.toHaveBeenCalled()
})

test('essential shared mounts materialize through the same optional mount primitive', async () => {
  const effects = context()
  const sub = await SubContainer.eager(
    effects,
    { imageId: 'main' },
    essential(),
    'test',
  )
  try {
    expect(effects.prepareSharedVolume).toHaveBeenCalledWith({
      ...options,
      subcontainer: 'guid',
    })
  } finally {
    await sub.destroy()
  }
})

test.each(['volume', 'assets', 'dependency', 'shared'] as const)(
  'shared mountpoints conflict with %s mounts',
  kind => {
    let mounts = essential()
    if (kind === 'volume')
      mounts = mounts.mountVolume({
        volumeId: 'main',
        subpath: null,
        mountpoint: options.mountpoint,
        readonly: false,
      })
    if (kind === 'assets')
      mounts = mounts.mountAssets({
        subpath: null,
        mountpoint: options.mountpoint,
      })
    if (kind === 'dependency')
      mounts = mounts.mountDependency({
        dependencyId: 'peer',
        volumeId: 'main',
        subpath: null,
        mountpoint: options.mountpoint,
        readonly: true,
      })
    if (kind === 'shared')
      mounts = mounts.mountSharedVolume({ ...options, id: 'another' })
    expect(() => mounts.build()).toThrow('cannot mount more than once')
  },
)

test.each([
  'media/music',
  '/media//music',
  '/media/./music',
  '/media/albums/../music',
])(
  'equivalent mount destination %s cannot overmount a declaration',
  mountpoint => {
    expect(() =>
      essential()
        .mountVolume({
          volumeId: 'main',
          mountpoint,
          subpath: null,
          readonly: false,
        })
        .build(),
    ).toThrow('cannot mount more than once')
  },
)

test.each(['eager', 'lazy'] as const)(
  '%s optional mount prepares its directory and preserves original mountpoint',
  async kind => {
    const effects = context()
    const sub =
      kind === 'eager'
        ? await SubContainer.eager<Manifest>(
            effects,
            { imageId: 'main' },
            null,
            'test',
          )
        : SubContainer.of<Manifest>(effects, { imageId: 'main' }, null, 'test')
    try {
      expect(sub.dependsOnSharedVolumes).toBe(false)
      await expect(sub.tryMountSharedVolume(options)).resolves.toEqual({
        kind: 'mounted',
      })
      expect(fs.mkdir).toHaveBeenCalledWith('/rootfs/media/music', {
        recursive: true,
      })
      expect(effects.prepareSharedVolume).toHaveBeenCalledWith({
        ...options,
        subcontainer: 'guid',
      })
      expect(effects.mount).not.toHaveBeenCalled()
      expect(sub.dependsOnSharedVolumes).toBe(true)
    } finally {
      await sub.destroy()
    }
  },
)

test('regular and shared volumes use the same subcontainer bind primitive', async () => {
  const effects = context()
  const sub = await SubContainer.eager<Manifest>(
    effects,
    { imageId: 'main' },
    Mounts.of<Manifest>().mountVolume({
      volumeId: 'main',
      mountpoint: '/data',
      subpath: null,
      readonly: false,
    }),
    'test',
  )
  try {
    await sub.tryMountSharedVolume(options)
    const binds = jest
      .mocked(subcontainers.execFile)
      .mock.calls.filter(
        ([command, args]) =>
          command === 'start-container' && args?.[0] === 'bind-mount',
      )
    expect(binds).toHaveLength(3)
    expect(binds[0][1]).toEqual(
      expect.arrayContaining([
        '--source',
        '/media/startos/volumes/main/',
        '--target',
        '/rootfs/data',
      ]),
    )
    expect(binds[1][1]).toEqual(
      expect.arrayContaining(['--source', placeholderPath, '--readonly']),
    )
    expect(binds[2][1]).toEqual(
      expect.arrayContaining([
        '--source',
        stagedPath,
        '--target',
        '/rootfs/media/music',
      ]),
    )
    expect(fs.mkdir).not.toHaveBeenCalledWith(stagedPath, { recursive: true })
    expect(fs.mkdir).not.toHaveBeenCalledWith(placeholderPath, {
      recursive: true,
    })
  } finally {
    await sub.destroy()
  }
})

test('shared remounts retain one read-only placeholder beneath data', async () => {
  const effects = context()
  jest
    .mocked(effects.prepareSharedVolume)
    .mockResolvedValueOnce(prepared({ kind: 'mounted' }))
    .mockResolvedValueOnce(
      prepared({ kind: 'unavailable', reason: 'sourceUnavailable' }),
    )
    .mockResolvedValueOnce(prepared({ kind: 'mounted' }))
  const sub = await SubContainer.eager<Manifest>(
    effects,
    { imageId: 'main' },
    null,
    'test',
  )
  try {
    await sub.tryMountSharedVolume(options)
    await sub.tryMountSharedVolume({
      ...options,
      mountpoint: '/media//./music',
    })
    await sub.tryMountSharedVolume(options)
    const calls = jest.mocked(subcontainers.execFile).mock.calls
    const binds = calls.filter(
      ([command, args]) =>
        command === 'start-container' && args?.[0] === 'bind-mount',
    )
    expect(binds.map(([, args]) => args?.[2])).toEqual([
      placeholderPath,
      stagedPath,
      stagedPath,
    ])
    expect(calls.filter(([command]) => command === 'umount')).toEqual([
      ['umount', ['--lazy', '/rootfs/media/music']],
    ])
  } finally {
    await sub.destroy()
  }
})

test('an in-flight shared bind holds the subcontainer until it completes', async () => {
  const effects = context()
  let release!: (value: T.PreparedSharedVolume) => void
  let started!: () => void
  const ready = new Promise<void>(resolve => (started = resolve))
  jest.mocked(effects.prepareSharedVolume).mockImplementation(() => {
    started()
    return new Promise(resolve => (release = resolve))
  })
  const sub = await SubContainer.eager<Manifest>(
    effects,
    { imageId: 'main' },
    null,
    'test',
  )
  const mounting = sub.tryMountSharedVolume(options)
  await ready
  await sub.destroy()
  expect(effects.subcontainer.destroyFs).not.toHaveBeenCalled()
  release(prepared({ kind: 'mounted' }))
  await mounting
  expect(effects.subcontainer.destroyFs).toHaveBeenCalledWith({ guid: 'guid' })
})

test.each(['sourceUnavailable', 'timeout'] as const)(
  'optional %s is an outcome and the affected workload is not launched',
  async reason => {
    const effects = context()
    jest
      .mocked(effects.prepareSharedVolume)
      .mockResolvedValue(prepared({ kind: 'unavailable', reason }))
    const sub = SubContainer.of<Manifest>(
      effects,
      { imageId: 'main' },
      null,
      'test',
    )
    const launch = jest.spyOn(sub, 'launch')
    try {
      const result = await sub.tryMountSharedVolume(options)
      if (result.kind === 'mounted') await sub.launch(['music-server'])
      expect(result).toEqual({ kind: 'unavailable', reason })
      expect(sub.dependsOnSharedVolumes).toBe(true)
      expect(launch).not.toHaveBeenCalled()
      expect(effects.subcontainer.destroyFs).not.toHaveBeenCalled()
    } finally {
      await sub.destroy()
    }
  },
)

test.each(['sourceUnavailable', 'timeout'] as const)(
  'essential %s rejects construction and tears down the subcontainer',
  async reason => {
    const effects = context()
    jest
      .mocked(effects.prepareSharedVolume)
      .mockResolvedValue(prepared({ kind: 'unavailable', reason }))
    await expect(
      SubContainer.eager(effects, { imageId: 'main' }, essential(), 'test'),
    ).rejects.toThrow(`Essential shared volume ${id} unavailable (${reason})`)
    expect(effects.subcontainer.destroyFs).toHaveBeenCalledWith({
      guid: 'guid',
    })
  },
)

test.each(['Authorization', 'InvalidRequest', 'Filesystem'])(
  'optional and essential mounts propagate %s errors',
  async message => {
    const effects = context()
    const error = new Error(message)
    jest.mocked(effects.prepareSharedVolume).mockRejectedValue(error)
    const sub = await SubContainer.eager<Manifest>(
      effects,
      { imageId: 'main' },
      null,
      'test',
    )
    try {
      await expect(sub.tryMountSharedVolume(options)).rejects.toBe(error)
    } finally {
      await sub.destroy()
    }
    await expect(
      SubContainer.eager(effects, { imageId: 'main' }, essential(), 'test'),
    ).rejects.toBe(error)
  },
)

test('invalid traversal cannot prepare a directory outside the subcontainer', async () => {
  const effects = context()
  const sub = await SubContainer.eager<Manifest>(
    effects,
    { imageId: 'main' },
    null,
    'test',
  )
  jest.mocked(fs.mkdir).mockClear()
  try {
    await expect(
      sub.tryMountSharedVolume({ ...options, mountpoint: '../../outside' }),
    ).rejects.toThrow('within the subcontainer')
    expect(fs.mkdir).not.toHaveBeenCalled()
    expect(effects.prepareSharedVolume).not.toHaveBeenCalled()
  } finally {
    await sub.destroy()
  }
})

test('dynamic recovery recreates an optional consumer and remounts after an equal-metadata invalidation', async () => {
  const effects = context()
  effects.child = () => ({ ...effects })
  effects.setHealth = jest.fn(async () => null)
  let callback: (() => void) | undefined
  jest.mocked(effects.getSharedVolumes).mockImplementation(async opts => {
    callback = opts.callback
    return [{ id, name: 'Media', readonly: true, availability: 'available' }]
  })
  let created = 0
  jest
    .mocked(effects.subcontainer.createFs)
    .mockImplementation(async () => [`/rootfs/${++created}`, `guid-${created}`])
  jest
    .mocked(effects.prepareSharedVolume)
    .mockResolvedValueOnce(
      prepared({ kind: 'unavailable', reason: 'sourceUnavailable' }),
    )
    .mockResolvedValue(prepared({ kind: 'mounted' }))
  const reconciler = Daemons.dynamic<Manifest>(effects, async ({ effects }) => {
    await getSharedVolumes(effects).const()
    const sub = SubContainer.of<Manifest>(
      effects,
      { imageId: 'main' },
      null,
      'optional',
    )
    return Daemons.of<Manifest>({ effects }).addDaemon('optional', {
      subcontainer: sub,
      exec: {
        fn: async (container, abort) => {
          await container!.tryMountSharedVolume(options)
          await new Promise<void>(resolve => {
            if (abort.aborted) resolve()
            else
              abort.addEventListener('abort', () => resolve(), { once: true })
          })
          return null
        },
      },
      ready: {
        display: 'Optional',
        fn: () => ({ result: 'success', message: null }),
        trigger: onceTrigger,
      },
      requires: [],
    })
  })
  const built = await reconciler.build()
  const waitForMounts = async (count: number) => {
    for (let i = 0; i < 100; i++) {
      if (jest.mocked(effects.prepareSharedVolume).mock.calls.length >= count)
        return
      await new Promise(resolve => setImmediate(resolve))
    }
    throw new Error(`expected ${count} shared-volume mount attempts`)
  }
  try {
    await waitForMounts(1)
    callback!()
    await waitForMounts(2)
    expect(effects.subcontainer.createFs).toHaveBeenCalledTimes(2)
    expect(effects.prepareSharedVolume).toHaveBeenNthCalledWith(1, {
      ...options,
      subcontainer: 'guid-1',
    })
    expect(effects.prepareSharedVolume).toHaveBeenNthCalledWith(2, {
      ...options,
      subcontainer: 'guid-2',
    })
    expect(effects.subcontainer.destroyFs).toHaveBeenCalledWith({
      guid: 'guid-1',
    })
  } finally {
    await built.term()
  }
})

test('listing preserves readonly, stable IDs and renamed sources through once, const, and watch', async () => {
  const effects = context()
  let callback: (() => void) | undefined
  let volumes: T.GrantedSharedVolume[] = [
    { id, name: 'Music', readonly: true, availability: 'unknown' },
  ]
  jest.mocked(effects.getSharedVolumes).mockImplementation(async opts => {
    callback = opts.callback
    return volumes
  })
  const reader = getSharedVolumes(effects)
  await expect(reader.once()).resolves.toEqual(volumes)
  expect(callback).toBeUndefined()
  await expect(reader.const()).resolves.toEqual(volumes)
  volumes = [{ id, name: 'Albums', readonly: false, availability: 'available' }]
  callback!()
  await new Promise(resolve => setImmediate(resolve))
  expect(effects.constRetry).toHaveBeenCalledTimes(1)
  const abort = new AbortController()
  const watch = reader.watch(abort.signal)
  expect((await watch.next()).value).toEqual(volumes)
  const next = watch.next()
  volumes = [{ ...volumes[0], availability: 'unavailable' }]
  callback!()
  expect((await next).value).toEqual(volumes)
  abort.abort()
  await expect(watch.next()).rejects.toMatchObject({ name: 'AbortedError' })
})

test('the SDK convenience APIs require manifest opt-in at compile time', () => {
  const manifest = setupManifest({
    id: 'test',
    title: '',
    license: '',
    packageRepo: '',
    upstreamRepo: '',
    marketingUrl: '',
    donationUrl: null,
    description: { short: '', long: '' },
    images: {},
    volumes: [],
  })
  const sdk = StartSdk.of().withManifest(manifest).build(true)
  const enabled = StartSdk.of()
    .withManifest(setupManifest({ ...manifest, sharedVolumes: true }))
    .build(true)
  const effects = context()
  if (false) {
    // @ts-expect-error Shared volumes require manifest opt-in.
    sdk.getSharedVolumes(effects)
    // @ts-expect-error Essential shared mounts require manifest opt-in.
    Mounts.of<typeof manifest>().mountSharedVolume(options)
    enabled.getSharedVolumes(effects)
    Mounts.of<typeof enabled.manifest>().mountSharedVolume(options)
  }
  expect(enabled.getSharedVolumes(effects)).toBeDefined()
})
