import { EventEmitter, getEventListeners } from 'node:events'
import * as cp from 'child_process'
import * as fs from 'fs/promises'
import { SubContainerEager } from '../util/SubContainer'
import { FileMounts } from '../util/fileMounts'
import { jsonEnv } from '../util/jsonEnv'

jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  spawn: jest.fn(),
}))

jest.mock('fs/promises', () => ({
  ...jest.requireActual('fs/promises'),
  readFile: jest.fn(),
}))

beforeEach(() => jest.mocked(fs.readFile).mockReset().mockResolvedValue('{}'))

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
  // Subscribe before exiting to avoid racing exec's awaits.
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
  sub.killLeader = async () => {}
  sub.fileMounts = new FileMounts()
  return sub as SubContainerEager<any>
}

describe('SubContainerEager command environment', () => {
  beforeEach(() => {
    ;(cp.spawn as unknown as jest.Mock).mockReset().mockReturnValue(fakeChild())
    jest.replaceProperty(process, 'env', {
      ...process.env,
      JSON_ENV_0: '{"LEAKED":"parent"}',
      JSON_ENV_1: 'stale',
      JSON_ENV_999: 'stale',
      WRAPPER_ONLY: 'inherited',
    })
  })

  afterEach(() => jest.restoreAllMocks())

  test.each(['exec', 'spawn', 'launch'] as const)(
    '%s transports image defaults and overrides only through numbered JSON_ENV chunks',
    async method => {
      jest.mocked(fs.readFile).mockResolvedValue(
        JSON.stringify({
          env: {
            IMAGE: 'first\nsecond=value\n',
            TOKEN: 'image',
            REMOVED: 'image',
          },
        }),
      )
      const overrides = {
        TOKEN: 'secret\nvalue=',
        REMOVED: undefined,
        EMPTY: '',
      }
      const commandOptions = { env: overrides, user: '1000', cwd: '/app' }
      await subContainer()[method](['true'], commandOptions)
      const [program, args, options] = (cp.spawn as unknown as jest.Mock).mock
        .calls[0]
      expect(program).toBe('start-container')
      expect(args).not.toContain(
        '--env-file=/media/startos/images/test-image.env',
      )
      expect(args.some((arg: string) => arg.startsWith('--env='))).toBe(false)
      expect(JSON.stringify(args)).not.toContain('secret')
      expect(JSON.parse(options.env.JSON_ENV_0)).toEqual({
        IMAGE: 'first\nsecond=value\n',
        TOKEN: 'secret\nvalue=',
        REMOVED: null,
        EMPTY: '',
      })
      expect(options.env.WRAPPER_ONLY).toBe('inherited')
      expect(options.env.TOKEN).toBeUndefined()
      expect(args).toEqual(
        expect.arrayContaining(['--user=1000', '--workdir=/app']),
      )
      expect(options.cwd).toBeUndefined()
      expect(commandOptions).toEqual({
        env: overrides,
        user: '1000',
        cwd: '/app',
      })
      expect(overrides).toHaveProperty('REMOVED', undefined)
    },
  )

  test.each(['exec', 'spawn', 'launch'] as const)(
    '%s falls back to the env file for legacy image metadata',
    async method => {
      jest.mocked(fs.readFile).mockResolvedValue('{}')
      await subContainer()[method](['true'], {
        env: { TOKEN: 'secret', REMOVE: undefined },
      })
      const [, args, options] = (cp.spawn as unknown as jest.Mock).mock.calls[0]
      expect(args).toContain('--env-file=/media/startos/images/test-image.env')
      expect(JSON.parse(options.env.JSON_ENV_0)).toEqual({
        TOKEN: 'secret',
        REMOVE: null,
      })
    },
  )

  test.each(['exec', 'spawn', 'launch'] as const)(
    '%s does not inherit wrapper JSON_ENV chunks when overrides are absent',
    async method => {
      jest.mocked(fs.readFile).mockResolvedValue('{"env":{}}')
      await subContainer()[method](['true'])
      const [, args, options] = (cp.spawn as unknown as jest.Mock).mock.calls[0]
      expect(args).not.toContain(
        '--env-file=/media/startos/images/test-image.env',
      )
      expect(options.env.JSON_ENV_0).toBe('{}')
      expect(options.env.JSON_ENV_1).toBeUndefined()
      expect(options.env.JSON_ENV_999).toBeUndefined()
    },
  )

  test.each(['exec', 'spawn', 'launch'] as const)(
    '%s preserves large Unicode environments across more than ten chunks',
    async method => {
      jest.mocked(fs.readFile).mockResolvedValue('{"env":{}}')
      const variables = {
        ...Object.fromEntries(
          Array.from({ length: 16 }, (_, i) => [
            `VALUE_${i}`,
            '🦀'.repeat(11_000),
          ]),
        ),
        MULTILINE: 'α\n=\"quoted\"\r\n',
        EMPTY: '',
        REMOVED: undefined,
      }
      await subContainer()[method](['true'], { env: variables })
      const [, args, options] = (cp.spawn as unknown as jest.Mock).mock.calls[0]
      const names = Object.keys(options.env).filter(name =>
        /^JSON_ENV_[0-9]+$/.test(name),
      )
      expect(names.length).toBeGreaterThan(10)
      expect(options.env.JSON_ENV_999).toBeUndefined()
      const chunks = Array.from(
        { length: names.length },
        (_, i) => options.env[`JSON_ENV_${i}`],
      )
      for (const chunk of chunks)
        expect(Buffer.byteLength(chunk)).toBeLessThanOrEqual(64 * 1024)
      expect(JSON.parse(chunks.join(''))).toEqual({
        ...variables,
        REMOVED: null,
      })
      expect(args.some((arg: string) => arg.startsWith('--env='))).toBe(false)
    },
  )

  test('numbered transport passes an environment larger than the per-string exec limit', () => {
    const environment = jsonEnv({
      A: 'x'.repeat(70 * 1024),
      B: 'y'.repeat(70 * 1024),
    })
    expect(
      cp.execFileSync(process.execPath, ['-e', 'process.stdout.write("ok")'], {
        env: environment,
        encoding: 'utf8',
      }),
    ).toBe('ok')
  })
})

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
    child.kill = jest.fn(() => child.emit('exit', null, 'SIGKILL'))
    spawned(child)
    const abort = new AbortController()

    const running = subContainer().exec(['sleep', '60'], {
      abort,
      timeout: null,
    })
    while (!child.listenerCount('exit')) {
      await new Promise(resolve => setImmediate(resolve))
    }
    abort.abort()

    await expect(running).resolves.toMatchObject({ exitSignal: 'SIGKILL' })
    expect(child.kill).toHaveBeenCalledWith('SIGKILL')
    expect(getEventListeners(abort.signal, 'abort')).toHaveLength(0)
  })
})
