import * as cp from 'child_process'
import { EventEmitter } from 'events'
import { statSync } from 'fs'
import * as fs from 'fs/promises'
import { tmpdir } from 'os'
import { join } from 'path'
import * as T from '@start9labs/start-core/types'
import { SubContainer } from '../util/SubContainer'
import * as subcontainers from '../util/SubContainer'

jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  spawn: jest.fn(),
}))

let dir: string
let rootfs: string
let resolver: string
let target: string
let effects: T.Effects

beforeEach(async () => {
  jest.clearAllMocks()
  dir = await fs.mkdtemp(join(tmpdir(), 'subcontainer-resolver-'))
  rootfs = join(dir, 'rootfs')
  resolver = join(rootfs, 'etc/resolv.conf')
  target = join(dir, 'image-resolver')
  await fs.mkdir(join(rootfs, 'etc'), { recursive: true })
  effects = {
    onLeaveContext: jest.fn(),
    subcontainer: {
      createFs: jest.fn(async () => [rootfs, 'guid']),
      destroyFs: jest.fn(async () => null),
    },
  } as unknown as T.Effects
  jest.spyOn(subcontainers, 'execFile').mockResolvedValue({
    stdout: '',
    stderr: '',
  })
  jest.spyOn(cp, 'spawn').mockImplementation(() => {
    expect(statSync(join(rootfs, 'proc')).isDirectory()).toBe(true)
    const leader = new EventEmitter()
    return Object.assign(leader, {
      kill: jest.fn(() => {
        queueMicrotask(() => leader.emit('exit', 0))
        return true
      }),
    }) as unknown as cp.ChildProcessWithoutNullStreams
  })
})

afterEach(async () => {
  jest.restoreAllMocks()
  await fs.rm(dir, { recursive: true, force: true })
})

it.each(['missing', 'regular', 'dangling symlink', 'symlink', 'hardlink'])(
  'installs the runtime resolver over a %s image resolver',
  async kind => {
    if (kind === 'regular') await fs.writeFile(resolver, 'image resolver\n')
    if (kind === 'dangling symlink') {
      await fs.symlink(
        '/nix/store/missing-image-root/etc/resolv.conf',
        resolver,
      )
    }
    if (kind === 'symlink' || kind === 'hardlink') {
      await fs.writeFile(target, 'image resolver\n')
      if (kind === 'symlink') await fs.symlink(target, resolver)
      else await fs.link(target, resolver)
    }

    const sub = await SubContainer.eager(
      effects,
      { imageId: 'main' },
      null,
      'test',
    )
    try {
      expect((await fs.lstat(resolver)).isFile()).toBe(true)
      expect(await fs.readFile(resolver)).toEqual(
        await fs.readFile('/etc/resolv.conf'),
      )
      if (kind === 'symlink' || kind === 'hardlink') {
        expect(await fs.readFile(target, 'utf8')).toBe('image resolver\n')
      }
    } finally {
      await sub.destroy()
    }
    expect(effects.subcontainer.destroyFs).toHaveBeenCalledTimes(1)
  },
)

it('creates a missing etc directory before installing the resolver', async () => {
  await fs.rmdir(join(rootfs, 'etc'))
  const sub = SubContainer.of(effects, { imageId: 'main' }, null, 'test')
  try {
    await sub.eager()
    expect(await fs.readFile(resolver)).toEqual(
      await fs.readFile('/etc/resolv.conf'),
    )
  } finally {
    await sub.destroy()
  }
})

it('destroys the filesystem when resolver installation fails', async () => {
  await fs.mkdir(resolver)
  await expect(
    SubContainer.eager(effects, { imageId: 'main' }, null, 'test'),
  ).rejects.toMatchObject({ code: 'ERR_FS_EISDIR' })
  expect(effects.subcontainer.destroyFs).toHaveBeenCalledWith({ guid: 'guid' })
  expect(effects.subcontainer.destroyFs).toHaveBeenCalledTimes(1)
  expect(subcontainers.execFile).not.toHaveBeenCalled()
})

it.each(['missing', 'existing'])(
  'prepares the %s proc mount point before launching the image',
  async kind => {
    if (kind === 'existing') await fs.mkdir(join(rootfs, 'proc'))
    const sub = await SubContainer.eager(
      effects,
      { imageId: 'main' },
      null,
      'test',
    )
    try {
      expect(cp.spawn).toHaveBeenCalledTimes(1)
    } finally {
      await sub.destroy()
    }
  },
)

it('destroys the filesystem without launching when proc preparation fails', async () => {
  await fs.writeFile(join(rootfs, 'proc'), '')
  await expect(
    SubContainer.eager(effects, { imageId: 'main' }, null, 'test'),
  ).rejects.toMatchObject({ code: 'EEXIST' })
  expect(effects.subcontainer.destroyFs).toHaveBeenCalledWith({ guid: 'guid' })
  expect(effects.subcontainer.destroyFs).toHaveBeenCalledTimes(1)
  expect(cp.spawn).not.toHaveBeenCalled()
})
