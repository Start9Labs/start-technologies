import { EventEmitter } from 'node:events'
import * as cp from 'child_process'
import { SubContainerEager } from '../util/SubContainer'

jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  spawn: jest.fn(),
}))

// cp.spawn is given no stdio override, so the child always gets a pipe for
// stdin. A command that reads to EOF therefore blocks until `timeout` fires and
// SIGKILLs it — the caller sees only a signal that points nowhere near stdin.
// exec must close stdin on every path, not only when it has something to write.
function fakeChild() {
  const child = new EventEmitter() as any
  child.pid = 4242
  child.kill = jest.fn()
  child.stdout = new EventEmitter()
  child.stderr = new EventEmitter()
  child.stdin = Object.assign(new EventEmitter(), {
    write: jest.fn((_data: unknown, cb: (e?: Error) => void) => cb()),
    end: jest.fn((cb?: () => void) => cb?.()),
  })
  // Exit as soon as exec subscribes, rather than racing the awaits it does
  // first (waitProc, reading the image metadata, closing stdin).
  const on = child.on.bind(child)
  child.on = (event: string, listener: (...a: any[]) => void) => {
    const result = on(event, listener)
    if (event === 'exit') setImmediate(() => child.emit('exit', 0, null))
    return result
  }
  return child
}

function subContainer() {
  const sub = Object.create(SubContainerEager.prototype)
  sub.imageId = 'test-image'
  sub.rootfs = '/tmp/does-not-exist'
  sub.waitProc = async () => {}
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
    expect(child.stdin.write).toHaveBeenCalledWith(
      'hunter2',
      expect.any(Function),
    )
    expect(child.stdin.end).toHaveBeenCalled()
  })

  test('does not throw when the caller inherited stdio', async () => {
    child.stdin = null
    await expect(run()).resolves.toBeDefined()
  })
})
