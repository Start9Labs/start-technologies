import { EventEmitter, getEventListeners } from 'node:events'
import * as cp from 'child_process'
import { SubContainerEager } from '../util/SubContainer'
import { FileMounts } from '../util/fileMounts'

jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  spawn: jest.fn(),
}))

// Commands that read stdin need EOF even when no input was supplied.
function fakeChild() {
  const child = new EventEmitter() as any
  child.pid = 4242
  child.kill = jest.fn()
  child.stdout = new EventEmitter()
  child.stderr = new EventEmitter()
  child.input = ''
  child.stdin = Object.assign(new EventEmitter(), {
    write: jest.fn((data: string, cb: (e?: Error) => void) => {
      child.input += data
      cb()
    }),
    end: jest.fn((data?: string | (() => void)) => {
      if (typeof data === 'function') data()
      else child.input += data ?? ''
    }),
  })
  // Complete asynchronously after the command's lifecycle listeners are attached.
  const on = child.on.bind(child)
  child.on = (event: string, listener: (...a: any[]) => void) => {
    const result = on(event, listener)
    if (event === 'exit' || event === 'close') {
      setImmediate(() => {
        child.emit('exit', 0, null)
        child.emit('close', 0, null)
      })
    }
    return result
  }
  return child
}

function subContainer() {
  const sub = Object.create(SubContainerEager.prototype)
  sub.imageId = 'test-image'
  sub.rootfs = '/tmp/does-not-exist'
  sub.waitProc = async () => {}
  sub.fileMounts = new FileMounts()
  return sub as SubContainerEager<any>
}

describe('SubContainerEager.exec stdin', () => {
  let child: ReturnType<typeof fakeChild>

  beforeEach(() => {
    child = fakeChild()
    ;(cp.spawn as unknown as jest.Mock).mockReset().mockReturnValue(child)
  })

  const run = (options?: Record<string, unknown>) =>
    subContainer().exec(['cat'], options as any)

  test('closes stdin when no input is given', async () => {
    await run()
    expect(child.stdin.write).not.toHaveBeenCalled()
    expect(child.stdin.end).toHaveBeenCalled()
  })

  test('closes stdin when input is the empty string', async () => {
    await run({ input: '' })
    expect(child.stdin.end).toHaveBeenCalled()
  })

  test('writes and then closes stdin when input is given', async () => {
    await run({ input: 'hunter2' })
    expect(child.input).toBe('hunter2')
    expect(child.stdin.end).toHaveBeenCalled()
  })

  test('does not throw when the caller inherited stdio', async () => {
    child.stdin = null
    await expect(run()).resolves.toBeDefined()
  })
})

describe('SubContainerEager.exec abort', () => {
  const spawned = (child: ReturnType<typeof fakeChild>) =>
    (cp.spawn as unknown as jest.Mock).mockReset().mockReturnValue(child)

  test('releases its abort listener once the command exits', async () => {
    const abort = new AbortController()
    for (let i = 0; i < 12; i++) {
      spawned(fakeChild())
      await subContainer().exec(['true'], { abort })
    }
    expect(getEventListeners(abort.signal, 'abort')).toHaveLength(0)
  })

  test('kills a running command when aborted', async () => {
    const child = fakeChild()
    child.on = EventEmitter.prototype.on
    child.kill = jest.fn(() => {
      child.emit('exit', null, 'SIGKILL')
      return child.emit('close', null, 'SIGKILL')
    })
    spawned(child)
    const abort = new AbortController()

    const running = subContainer().exec(['sleep', '60'], {
      abort,
      timeout: null,
    })
    while (!child.listenerCount('close')) {
      await new Promise(resolve => setImmediate(resolve))
    }
    abort.abort()

    await expect(running).resolves.toMatchObject({ exitSignal: 'SIGKILL' })
    expect(child.kill).toHaveBeenCalledWith('SIGKILL')
    expect(getEventListeners(abort.signal, 'abort')).toHaveLength(0)
  })
})
