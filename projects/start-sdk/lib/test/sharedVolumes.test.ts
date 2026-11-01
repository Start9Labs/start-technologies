import * as cp from 'child_process'
import { EventEmitter } from 'events'
import * as fs from 'fs/promises'
import * as T from '@start9labs/start-core/types'
import { getSharedVolumes } from '@start9labs/start-core/util'
import { StartSdk } from '../StartSdk'
import { Mounts } from '../mainFn/Mounts'
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
    mountSharedVolume: jest.fn(
      async (): Promise<T.SharedVolumeMountResult> => ({ kind: 'mounted' }),
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
    expect(effects.mountSharedVolume).toHaveBeenCalledWith({
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
      await expect(sub.tryMountSharedVolume(options)).resolves.toEqual({
        kind: 'mounted',
      })
      expect(fs.mkdir).toHaveBeenCalledWith('/rootfs/media/music', {
        recursive: true,
      })
      expect(effects.mountSharedVolume).toHaveBeenCalledWith({
        ...options,
        subcontainer: 'guid',
      })
      expect(effects.mount).not.toHaveBeenCalled()
    } finally {
      await sub.destroy()
    }
  },
)

test.each(['sourceUnavailable', 'timeout'] as const)(
  'optional %s is an outcome and the affected workload is not launched',
  async reason => {
    const effects = context()
    jest
      .mocked(effects.mountSharedVolume)
      .mockResolvedValue({ kind: 'unavailable', reason })
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
      .mocked(effects.mountSharedVolume)
      .mockResolvedValue({ kind: 'unavailable', reason })
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
    jest.mocked(effects.mountSharedVolume).mockRejectedValue(error)
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
    expect(effects.mountSharedVolume).not.toHaveBeenCalled()
  } finally {
    await sub.destroy()
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
