import { EventEmitter, getEventListeners } from 'node:events'
import * as cp from 'child_process'
import { HealthDaemon } from '../mainFn/HealthDaemon'
import { SubContainerEager } from '../util/SubContainer'

jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  spawn: jest.fn(),
}))

function fakeChild(mode = 'fast') {
  const child = new EventEmitter() as any
  child.pid = 4242
  child.stdout = new EventEmitter()
  child.stderr = new EventEmitter()
  child.stdin = new EventEmitter()
  let exited = false
  function exit(code: number | null, signal: NodeJS.Signals | null = null) {
    if (exited) return false
    exited = true
    child.emit('exit', code, signal)
    setImmediate(() => {
      child.stdout.emit('data', 'tail')
      child.stderr.emit('data', 'diagnostic')
      child.emit('close', code, signal)
    })
    return true
  }
  child.kill = jest.fn((signal: NodeJS.Signals) => exit(null, signal))
  child.stdin.write = (_input: unknown, callback?: () => void) => callback?.()
  child.stdin.end = (...args: unknown[]) => {
    if (mode === 'hung-stdin') return
    process.nextTick(() => {
      if (mode === 'spawn-error') {
        child.emit('error', new Error('spawn ENOENT'))
        exit(-2)
      } else if (mode === 'stdin-error') {
        child.stdin.emit('error', new Error('write EPIPE'))
      } else {
        exit(mode === 'nonzero' ? 1 : 0)
      }
      const callback = args.at(-1)
      if (typeof callback === 'function') callback()
    })
  }
  // Unobserved errors must fail the assertion instead of crashing the runner.
  child.on('error', () => {})
  child.stdin.on('error', () => {})
  return child
}

function subject(mode = 'fast') {
  const child = fakeChild(mode)
  ;(cp.spawn as unknown as jest.Mock).mockReturnValue(child)
  const sub = Object.create(SubContainerEager.prototype)
  Object.assign(sub, {
    imageId: 'test-image',
    rootfs: '/unused',
    waitProc: async () => {},
    fileMounts: { sync: async () => {} },
  })
  return { child, sub: sub as SubContainerEager<any> }
}

async function bounded<T>(promise: Promise<T>): Promise<T> {
  let timer: NodeJS.Timeout | undefined
  try {
    return await Promise.race([
      promise,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error('command did not settle')),
          250,
        )
      }),
    ])
  } finally {
    clearTimeout(timer)
  }
}

test('captures output and completes when exit precedes the stdin callback', async () => {
  const { sub } = subject()
  const result = await bounded(sub.exec(['true'], { timeout: 25 }))
  expect(result).toMatchObject({
    exitCode: 0,
    exitSignal: null,
    stdout: 'tail',
    stderr: 'diagnostic',
  })
})

test('health checks can terminate while fast commands finish', async () => {
  const { sub } = subject()
  ;(cp.spawn as unknown as jest.Mock).mockImplementation(() => fakeChild())
  let entered!: () => void
  const started = new Promise<void>(resolve => {
    entered = resolve
  })
  const health = new HealthDaemon(
    null,
    [],
    'folders',
    {
      display: 'Website Folders',
      trigger: async function* () {
        yield
      },
      fn: async () => {
        entered()
        await Promise.all(
          Array.from({ length: 36 }, () => sub.exec(['true'], { timeout: 25 })),
        )
        return { result: 'success', message: 'All folders found' }
      },
    },
    { setHealth: async () => {} } as any,
  )
  await health.updateStatus()
  await started
  await bounded(health.term())
})

test('the deadline covers a stdin callback that never arrives', async () => {
  const { sub } = subject('hung-stdin')
  await expect(
    bounded(sub.exec(['cat'], { timeout: 10 })),
  ).resolves.toMatchObject({
    exitSignal: 'SIGKILL',
    timedOutAfter: 10,
  })
})

test('spawn failure rejects even before stdin has finished', async () => {
  const { sub } = subject('spawn-error')
  await expect(bounded(sub.exec(['missing'], { timeout: 25 }))).rejects.toThrow(
    'ENOENT',
  )
})

test('stdin failure rejects and releases the abort listener', async () => {
  const { sub, child } = subject('stdin-error')
  const abort = new AbortController()
  await expect(
    bounded(sub.exec(['cat'], { timeout: 25, abort })),
  ).rejects.toThrow('EPIPE')
  expect(child.kill).toHaveBeenCalledWith('SIGKILL')
  expect(getEventListeners(abort.signal, 'abort')).toHaveLength(0)
})

test('preserves a nonzero exit and throw()', async () => {
  const { sub } = subject('nonzero')
  const result = await bounded(sub.exec(['false'], { timeout: 25 }))
  expect(result.exitCode).toBe(1)
  expect(() => result.throw()).toThrow()
})

test('an already aborted command is killed without waiting for its deadline', async () => {
  const { sub } = subject('hung-stdin')
  const abort = new AbortController()
  abort.abort()
  const result = await bounded(sub.exec(['cat'], { timeout: 25, abort }))
  expect(result).toMatchObject({ exitSignal: 'SIGKILL', timedOutAfter: null })
  expect(getEventListeners(abort.signal, 'abort')).toHaveLength(0)
})
