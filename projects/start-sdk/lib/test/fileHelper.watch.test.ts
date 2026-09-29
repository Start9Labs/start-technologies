import fs from 'node:fs/promises'
import { WatchOptions } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { Effects } from '@start9labs/start-core/Effects'
import { AbortedError } from '@start9labs/start-core/util/AbortedError'
import { FileHelper } from '../util/fileHelper'

const effects = { isInContext: true } as Effects
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
