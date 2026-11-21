import fs from 'node:fs/promises'
import { WatchOptions } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { Effects } from '@start9labs/start-core/Effects'
import { AbortedError } from '@start9labs/start-core/util/AbortedError'
import * as z from '@start9labs/start-core/z'
import { FileHelper } from '../util/fileHelper'

const effects = {
  isInContext: true,
  onLeaveContext: (_fn: () => void | null | undefined) => {},
} as Effects
const realWatch = fs.watch

const observeWatch = () => {
  let started!: () => void
  const watching = new Promise<void>(resolve => (started = resolve))
  const spy = jest.spyOn(fs, 'watch').mockImplementation(((
    filename,
    options,
  ) => {
    const iterator = realWatch(filename, options as WatchOptions)
    const next = iterator.next.bind(iterator)
    iterator.next = (...args) => {
      const pending = next(...args)
      started()
      return pending
    }
    return iterator
  }) as typeof fs.watch)
  return { watching, spy }
}

describe('FileHelper.waitFor cancellation', () => {
  test.each([
    'config.txt',
    'parent/config.txt',
    'parent/child/config.txt',
    'existing.txt',
  ])('aborts a watch of %s', async subpath => {
    const dir = await fs.mkdtemp(join(tmpdir(), 'file-wait-'))
    const path = join(dir, subpath)
    const abort = new AbortController()
    const initial = subpath === 'existing.txt' ? 'pending' : null
    if (initial !== null) await fs.writeFile(path, initial)
    const { watching, spy } = observeWatch()
    let predicateCalled!: () => void
    const firstRead = new Promise<void>(r => (predicateCalled = r))
    const predicate = jest.fn(value => {
      predicateCalled()
      return value === 'created'
    })
    const wait = Promise.resolve(
      FileHelper.string(path).read().waitFor(effects, predicate, abort.signal),
    )
    let timeout: NodeJS.Timeout | undefined
    try {
      await Promise.all([watching, firstRead])
      expect(predicate).toHaveBeenCalledWith(initial)
      abort.abort()
      await expect(
        Promise.race([
          wait,
          new Promise(resolve => {
            timeout = setTimeout(() => resolve('still waiting'), 1000)
          }),
        ]),
      ).rejects.toThrow(AbortedError)
    } finally {
      clearTimeout(timeout)
      await fs.mkdir(dirname(path), { recursive: true })
      await fs.writeFile(path, 'created')
      await wait.catch(() => {})
      spy.mockRestore()
      await fs.rm(dir, { recursive: true, force: true })
    }
  })

  test.each([false, true])(
    'reads a file after a change (exists: %s)',
    async exists => {
      const dir = await fs.mkdtemp(join(tmpdir(), 'file-wait-'))
      const path = join(dir, 'config.txt')
      if (exists) await fs.writeFile(path, 'pending')
      const abort = new AbortController()
      const { watching, spy } = observeWatch()
      const wait = Promise.resolve(
        FileHelper.string(path)
          .read()
          .waitFor(effects, value => value === 'created', abort.signal),
      )
      try {
        await watching
        await fs.writeFile(path, 'created')
        await expect(wait).resolves.toBe('created')
      } finally {
        abort.abort()
        await wait.catch(() => {})
        spy.mockRestore()
        await fs.rm(dir, { recursive: true, force: true })
      }
    },
  )

  test('an already-aborted signal starts no filesystem watch', async () => {
    const spy = jest.spyOn(fs, 'watch')
    try {
      await expect(
        FileHelper.string('missing.txt')
          .read()
          .waitFor(effects, value => value !== null, AbortSignal.abort()),
      ).rejects.toThrow(AbortedError)
      expect(spy).not.toHaveBeenCalled()
    } finally {
      spy.mockRestore()
    }
  })
})

describe('FileHelper.watch', () => {
  const next = <T>(gen: AsyncGenerator<T>) =>
    Promise.race([
      gen.next().then(r => r.value),
      new Promise(resolve => setTimeout(() => resolve('still waiting'), 1000)),
    ])

  test('sees a write made while the consumer holds a value', async () => {
    const dir = await fs.mkdtemp(join(tmpdir(), 'file-watch-'))
    const path = join(dir, 'config.txt')
    await fs.writeFile(path, 'a')
    const abort = new AbortController()
    const gen = FileHelper.string(path).read().watch(effects, abort.signal)
    try {
      expect((await gen.next()).value).toBe('a')
      await fs.writeFile(path, 'b')
      expect(await next(gen)).toBe('b')
    } finally {
      abort.abort()
      await fs.rm(dir, { recursive: true, force: true })
    }
  })

  test('follows successive atomic replacements', async () => {
    const dir = await fs.mkdtemp(join(tmpdir(), 'file-watch-'))
    const path = join(dir, 'config.txt')
    const file = FileHelper.string(path)
    await file.write(effects, 'a')
    const abort = new AbortController()
    const gen = file.read().watch(effects, abort.signal)
    try {
      expect((await gen.next()).value).toBe('a')
      for (const value of ['b', 'c', 'd']) {
        await file.write(effects, value)
        expect(await next(gen)).toBe(value)
      }
    } finally {
      abort.abort()
      await fs.rm(dir, { recursive: true, force: true })
    }
  })

  test('returns while the consumer holds a value', async () => {
    const dir = await fs.mkdtemp(join(tmpdir(), 'file-watch-'))
    const path = join(dir, 'config.txt')
    await fs.writeFile(path, 'a')
    const gen = FileHelper.string(path).read().watch(effects)
    try {
      await gen.next()
      expect(
        await Promise.race([
          gen.return(undefined as never).then(() => 'returned'),
          new Promise(resolve => setTimeout(() => resolve('hung'), 1000)),
        ]),
      ).toBe('returned')
    } finally {
      await fs.rm(dir, { recursive: true, force: true })
    }
  })
})

describe('FileHelper watch retry backoff', () => {
  const BACKOFF_SCHEDULE = [
    100, 200, 400, 800, 1600, 3200, 6400, 12800, 25600, 30000, 30000,
  ]

  const makeEmfile = (target: unknown) =>
    Object.assign(new Error(`EMFILE: too many open files, watch '${target}'`), {
      code: 'EMFILE',
    })

  const makeFailingWatchIterator = (
    target: unknown,
    signal?: AbortSignal,
  ): AsyncIterableIterator<fs.FileChangeInfo<string>> => ({
    [Symbol.asyncIterator]() {
      return this
    },
    next() {
      if (signal?.aborted) {
        const err = new Error('The operation was aborted')
        err.name = 'AbortError'
        return Promise.reject(err)
      }
      return Promise.reject(makeEmfile(target))
    },
    return() {
      return Promise.resolve({ value: undefined, done: true })
    },
  })

  const mockFailingWatch = (
    override?: (
      call: number,
      target: unknown,
      signal?: AbortSignal,
    ) => AsyncIterableIterator<fs.FileChangeInfo<string>> | undefined,
  ) => {
    let calls = 0
    return jest.spyOn(fs, 'watch').mockImplementation(((target, options) => {
      calls += 1
      const signal = (options as WatchOptions | undefined)?.signal
      return (
        override?.(calls, target, signal) ??
        makeFailingWatchIterator(target, signal)
      )
    }) as typeof fs.watch)
  }

  const createErrorTracker = () => {
    const waiters = new Map<number, () => void>()
    const spy = jest.spyOn(console, 'error').mockImplementation(() => {
      const count = spy.mock.calls.length
      const resolve = waiters.get(count)
      if (resolve) {
        waiters.delete(count)
        resolve()
      }
    })
    const waitForErrors = (count: number) =>
      spy.mock.calls.length >= count
        ? Promise.resolve()
        : new Promise<void>(resolve => waiters.set(count, resolve))
    return { spy, waitForErrors }
  }

  const flushImmediate = () =>
    new Promise<void>(resolve => setImmediate(resolve))

  afterEach(() => {
    jest.useRealTimers()
    jest.restoreAllMocks()
  })

  test.each([
    { name: 'existing file', exists: true },
    { name: 'missing file (onCreated)', exists: false },
  ])(
    'backs off exponentially up to 30s on persistent EMFILE ($name)',
    async ({ exists }) => {
      const dir = await fs.mkdtemp(join(tmpdir(), 'file-backoff-'))
      const path = join(dir, 'config.txt')
      if (exists) await fs.writeFile(path, 'initial')

      jest.useFakeTimers({ doNotFake: ['nextTick', 'setImmediate'] })
      const { spy: errorSpy, waitForErrors } = createErrorTracker()
      const watchSpy = mockFailingWatch()

      const abort = new AbortController()
      const wait = FileHelper.string(path)
        .read()
        .waitFor(effects, value => value === 'updated', abort.signal)

      try {
        await waitForErrors(1)
        await flushImmediate()
        expect(watchSpy).toHaveBeenCalledTimes(1)
        expect(errorSpy).toHaveBeenCalledTimes(1)
        expect(jest.getTimerCount()).toBe(1)

        for (let i = 0; i < BACKOFF_SCHEDULE.length; i++) {
          const delay = BACKOFF_SCHEDULE[i]
          const expectedBefore = i + 1
          await jest.advanceTimersByTimeAsync(delay - 1)
          await flushImmediate()
          expect(jest.getTimerCount()).toBe(1)
          expect(watchSpy).toHaveBeenCalledTimes(expectedBefore)
          expect(errorSpy).toHaveBeenCalledTimes(expectedBefore)

          await jest.advanceTimersByTimeAsync(1)
          expect(jest.getTimerCount()).toBe(0)
          await waitForErrors(expectedBefore + 1)
          await flushImmediate()
          expect(watchSpy).toHaveBeenCalledTimes(expectedBefore + 1)
          expect(errorSpy).toHaveBeenCalledTimes(expectedBefore + 1)
          expect(jest.getTimerCount()).toBe(1)
        }
      } finally {
        abort.abort()
        await wait.catch(() => {})
        await fs.rm(dir, { recursive: true, force: true })
      }
    },
  )

  test.each([
    { name: 'existing file change', exists: true },
    { name: 'missing file (onCreated)', exists: false },
  ])('resets backoff to 100ms after recovery ($name)', async ({ exists }) => {
    const dir = await fs.mkdtemp(join(tmpdir(), 'file-backoff-reset-'))
    const path = join(dir, 'config.txt')
    if (exists) await fs.writeFile(path, 'v1')

    jest.useFakeTimers({ doNotFake: ['nextTick', 'setImmediate'] })
    const { spy: errorSpy, waitForErrors } = createErrorTracker()

    let deliverEvent!: () => void
    let watchReady!: () => void
    const thirdWatchReady = new Promise<void>(resolve => (watchReady = resolve))

    const watchSpy = mockFailingWatch(call => {
      if (call !== 3) return undefined
      return {
        [Symbol.asyncIterator]() {
          return this
        },
        next() {
          watchReady()
          return new Promise<IteratorResult<fs.FileChangeInfo<string>>>(
            resolve => {
              deliverEvent = () =>
                resolve({
                  value: {
                    eventType: exists ? 'change' : 'rename',
                    filename: 'config.txt',
                  },
                  done: false,
                })
            },
          )
        },
        return() {
          return Promise.resolve({ value: undefined, done: true })
        },
      }
    })

    const abort = new AbortController()
    const gen = FileHelper.string(path).read().watch(effects, abort.signal)
    let pendingNext: Promise<unknown> | undefined

    try {
      expect((await gen.next()).value).toBe(exists ? 'v1' : null)
      const nextRead = gen.next()
      pendingNext = nextRead

      await waitForErrors(1)
      await flushImmediate()
      expect(jest.getTimerCount()).toBe(1)
      await jest.advanceTimersByTimeAsync(100)
      await waitForErrors(2)
      await flushImmediate()
      expect(jest.getTimerCount()).toBe(1)

      await jest.advanceTimersByTimeAsync(200)
      await thirdWatchReady
      await flushImmediate()
      await fs.writeFile(path, 'v2')
      deliverEvent()

      expect((await nextRead).value).toBe('v2')
      const afterReset = gen.next()
      pendingNext = afterReset

      await waitForErrors(3)
      await flushImmediate()
      expect(watchSpy).toHaveBeenCalledTimes(4)
      expect(jest.getTimerCount()).toBe(1)

      await jest.advanceTimersByTimeAsync(99)
      await flushImmediate()
      expect(jest.getTimerCount()).toBe(1)
      expect(watchSpy).toHaveBeenCalledTimes(4)

      await jest.advanceTimersByTimeAsync(1)
      expect(jest.getTimerCount()).toBe(0)
      await waitForErrors(4)
      await flushImmediate()
      expect(watchSpy).toHaveBeenCalledTimes(5)
      expect(errorSpy).toHaveBeenCalledTimes(4)

      abort.abort()
      await expect(afterReset).rejects.toThrow(AbortedError)
    } finally {
      abort.abort()
      await pendingNext?.catch(() => {})
      await gen.return(undefined as never).catch(() => {})
      await fs.rm(dir, { recursive: true, force: true })
    }
  })

  test('aborts waitFor and watch immediately during backoff sleep without logging AbortError', async () => {
    const dir = await fs.mkdtemp(join(tmpdir(), 'file-backoff-abort-'))
    const path = join(dir, 'config.txt')
    await fs.writeFile(path, 'initial')

    jest.useFakeTimers({ doNotFake: ['nextTick', 'setImmediate'] })
    const { spy: errorSpy, waitForErrors } = createErrorTracker()
    mockFailingWatch()

    const waitAbort = new AbortController()
    const watchAbort = new AbortController()
    const wait = FileHelper.string(path)
      .read()
      .waitFor(effects, value => value === 'updated', waitAbort.signal)
    const gen = FileHelper.string(path).read().watch(effects, watchAbort.signal)
    let watchNext: Promise<unknown> | undefined

    try {
      expect((await gen.next()).value).toBe('initial')
      watchNext = gen.next()

      await waitForErrors(2)
      await flushImmediate()
      expect(jest.getTimerCount()).toBe(2)

      waitAbort.abort()
      watchAbort.abort()

      await expect(wait).rejects.toThrow(AbortedError)
      await expect(watchNext).rejects.toThrow(AbortedError)
      expect(jest.getTimerCount()).toBe(0)
      expect(errorSpy).toHaveBeenCalledTimes(2)
    } finally {
      waitAbort.abort()
      watchAbort.abort()
      await wait.catch(() => {})
      await watchNext?.catch(() => {})
      await gen.return(undefined as never).catch(() => {})
      await fs.rm(dir, { recursive: true, force: true })
    }
  })

  test('terminates immediately on effects.onLeaveContext during backoff sleep without accumulating listeners', async () => {
    const dir = await fs.mkdtemp(join(tmpdir(), 'file-backoff-leave-'))
    const path = join(dir, 'config.txt')
    await fs.writeFile(path, 'initial')

    jest.useFakeTimers({ doNotFake: ['nextTick', 'setImmediate'] })
    const { spy: errorSpy, waitForErrors } = createErrorTracker()
    mockFailingWatch()

    const leaveCallbacks: (() => void | null | undefined)[] = []
    const ctxEffects = {
      isInContext: true,
      onLeaveContext: (fn: () => void | null | undefined) => {
        leaveCallbacks.push(fn)
      },
    } as Effects

    const gen = FileHelper.string(path).read().watch(ctxEffects)
    let watchNext: Promise<unknown> | undefined

    try {
      expect((await gen.next()).value).toBe('initial')
      watchNext = gen.next()

      await waitForErrors(1)
      await flushImmediate()
      expect(jest.getTimerCount()).toBe(1)
      await jest.advanceTimersByTimeAsync(100)
      await waitForErrors(2)
      await flushImmediate()

      expect(leaveCallbacks).toHaveLength(1)
      expect(jest.getTimerCount()).toBe(1)

      ctxEffects.isInContext = false
      for (const fn of leaveCallbacks) fn()

      await expect(watchNext).rejects.toThrow(AbortedError)
      expect(jest.getTimerCount()).toBe(0)
      expect(errorSpy).toHaveBeenCalledTimes(2)
    } finally {
      ctxEffects.isInContext = false
      for (const fn of leaveCallbacks) fn()
      await watchNext?.catch(() => {})
      await gen.return(undefined as never).catch(() => {})
      await fs.rm(dir, { recursive: true, force: true })
    }
  })

  test('propagates doRead errors without retrying with backoff', async () => {
    const dir = await fs.mkdtemp(join(tmpdir(), 'file-backoff-read-err-'))
    const invalidJsonPath = join(dir, 'bad.json')
    const badSchemaPath = join(dir, 'schema.json')
    await fs.writeFile(invalidJsonPath, '{invalid json')
    await fs.writeFile(badSchemaPath, JSON.stringify({ count: 'not-a-number' }))

    const schema = z.object({ count: z.number() })
    const errorSpy = jest.spyOn(console, 'error').mockImplementation(() => {})
    const watchSpy = jest.spyOn(fs, 'watch')

    try {
      await expect(
        FileHelper.json(invalidJsonPath, schema)
          .read()
          .waitFor(effects, () => true),
      ).rejects.toThrow(SyntaxError)

      await expect(
        FileHelper.json(badSchemaPath, schema)
          .read()
          .waitFor(effects, () => true),
      ).rejects.toThrow()

      expect(watchSpy).toHaveBeenCalledTimes(2)
      expect(errorSpy).not.toHaveBeenCalled()
    } finally {
      await fs.rm(dir, { recursive: true, force: true })
    }
  })
})
