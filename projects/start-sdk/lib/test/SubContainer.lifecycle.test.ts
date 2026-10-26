import * as cp from 'child_process'
import { EventEmitter } from 'events'
import * as fs from 'fs/promises'
import * as T from '@start9labs/start-core/types'
import { Daemons } from '../mainFn/Daemons'
import { Mounts } from '../mainFn/Mounts'
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

function deferred<A>() {
  let resolve!: (value: A) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<A>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

function context() {
  const callbacks: (() => void)[] = []
  const children = new Map<string, ReturnType<typeof context>>()
  const leave = () => {
    for (const child of children.values()) child.leave()
    children.clear()
    for (const callback of callbacks.splice(0)) callback()
  }
  const effects = {
    child: (name: string) => {
      children.get(name)?.leave()
      const child = context()
      children.set(name, child)
      return child.effects
    },
    onLeaveContext: (callback: () => void) => callbacks.push(callback),
    setHealth: async () => null,
    clearCallbacks: async () => null,
    subcontainer: {
      createFs: jest.fn(async () => ['/rootfs', 'guid']),
      destroyFs: jest.fn(async () => null),
    },
    mount: jest.fn(async () => null),
  } as unknown as T.Effects
  return { effects, leave }
}

const mounts = Mounts.of<T.SDKManifest>().mountDependency({
  dependencyId: 'dependency',
  volumeId: 'main',
  subpath: null,
  mountpoint: '/data',
  readonly: false,
})

const tick = () => new Promise<void>(resolve => setImmediate(resolve))

beforeEach(() => {
  jest.spyOn(fs, 'stat').mockRejectedValue(new Error('ENOENT'))
  jest.spyOn(fs, 'mkdir').mockResolvedValue(undefined)
  jest.spyOn(fs, 'copyFile').mockResolvedValue(undefined)
  jest.spyOn(fs, 'rm').mockResolvedValue(undefined)
  jest.spyOn(subcontainers, 'execFile').mockResolvedValue({
    stdout: '',
    stderr: '',
  })
  jest.spyOn(cp, 'spawn').mockImplementation(() => {
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

describe('SubContainer context ownership during materialization', () => {
  it.each([
    'before createFs',
    'during createFs',
    'during mounts',
    'after mounts',
  ])(
    'honors detach %s and still respects explicit destroy and holds',
    async timing => {
      const { effects, leave } = context()
      const created = deferred<[string, T.Guid]>()
      jest
        .spyOn(effects.subcontainer, 'createFs')
        .mockReturnValue(created.promise)
      const mounting = deferred<void>()
      const mounted = deferred<null>()
      jest.spyOn(effects, 'mount').mockImplementation(async () => {
        mounting.resolve()
        return mounted.promise
      })
      const sub = SubContainer.of(effects, { imageId: 'main' }, mounts, 'test')
      if (timing === 'before createFs') sub.detach()
      const materialized = sub.eager()
      if (timing === 'during createFs') sub.detach()
      created.resolve(['/rootfs', 'guid'])
      await mounting.promise
      if (timing === 'during mounts') sub.detach()
      if (timing !== 'after mounts') {
        leave()
        await tick()
        expect(effects.subcontainer.destroyFs).not.toHaveBeenCalled()
      }
      mounted.resolve(null)
      const eager = await materialized
      if (timing === 'after mounts') {
        sub.detach()
        leave()
      }
      await tick()
      expect(effects.subcontainer.destroyFs).not.toHaveBeenCalled()
      expect(await sub.eager()).toBe(eager)
      expect(await sub.rootfs).toBe('/rootfs')

      const release = sub.hold()
      await tick()
      await sub.destroy()
      expect(effects.subcontainer.destroyFs).not.toHaveBeenCalled()
      await release()
      expect(effects.subcontainer.destroyFs).toHaveBeenCalledTimes(1)
    },
  )

  it('preserves a pending destroy when detached during mounting', async () => {
    const { effects, leave } = context()
    const mounting = deferred<void>()
    const mounted = deferred<null>()
    jest.spyOn(effects, 'mount').mockImplementation(async () => {
      mounting.resolve()
      return mounted.promise
    })
    const sub = SubContainer.of(effects, { imageId: 'main' }, mounts, 'test')
    const release = sub.hold()
    await mounting.promise
    const destroyed = sub.destroy()
    sub.detach()
    leave()
    await tick()
    expect(effects.subcontainer.destroyFs).not.toHaveBeenCalled()
    await release()
    mounted.resolve(null)
    await destroyed
    expect(effects.subcontainer.destroyFs).toHaveBeenCalledTimes(1)
  })

  it('keeps context cleanup armed for an attached container', async () => {
    const { effects, leave } = context()
    const sub = SubContainer.of(effects, { imageId: 'main' }, mounts, 'test')
    await sub.eager()
    leave()
    await tick()
    expect(effects.subcontainer.destroyFs).toHaveBeenCalledTimes(1)
  })

  it('cleans up failed mounts and preserves detach intent on retry', async () => {
    const { effects, leave } = context()
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const failure = new Error('mount failed')
    jest.spyOn(effects, 'mount').mockRejectedValueOnce(failure)
    const sub = SubContainer.of(effects, { imageId: 'main' }, mounts, 'test')
    sub.detach()
    await expect(sub.eager()).rejects.toBe(failure)
    expect(effects.subcontainer.destroyFs).toHaveBeenCalledTimes(1)
    await sub.eager()
    leave()
    await tick()
    expect(effects.subcontainer.destroyFs).toHaveBeenCalledTimes(1)
    await sub.destroy()
    expect(effects.subcontainer.destroyFs).toHaveBeenCalledTimes(2)
    expect(logged).toHaveBeenCalledWith(failure)
  })

  it('keeps a dynamic oneshot alive through a mid-mount reconcile until term', async () => {
    const { effects } = context()
    const mounting = deferred<void>()
    const mounted = deferred<null>()
    const reran = deferred<void>()
    const executing = deferred<void>()
    const finished = deferred<null>()
    let firstEffects!: T.Effects
    let firstSub!: ReturnType<typeof SubContainer.of>
    let runs = 0
    const reconciler = Daemons.dynamic(effects, async ({ effects }) => {
      const sub = SubContainer.of(effects, { imageId: 'main' }, mounts, 'test')
      if (++runs === 1) {
        firstEffects = effects
        firstSub = sub
        jest.spyOn(effects, 'mount').mockImplementation(async () => {
          mounting.resolve()
          return mounted.promise
        })
      } else {
        reran.resolve()
      }
      return Daemons.of({ effects }).addOneshot('build', {
        subcontainer: sub,
        exec: {
          fn: async sub => {
            await sub.rootfs
            executing.resolve()
            return finished.promise
          },
        },
        requires: [],
      })
    })
    const running = await reconciler.build()
    await mounting.promise
    firstEffects.constRetry!()
    await reran.promise
    await tick()
    expect(firstEffects.subcontainer.destroyFs).not.toHaveBeenCalled()
    mounted.resolve(null)
    await executing.promise
    expect(await firstSub.rootfs).toBe('/rootfs')
    expect(firstEffects.subcontainer.createFs).toHaveBeenCalledTimes(1)
    expect(firstEffects.subcontainer.destroyFs).not.toHaveBeenCalled()
    finished.resolve(null)
    await running.term()
    expect(firstEffects.subcontainer.destroyFs).toHaveBeenCalledTimes(1)
  })
})
