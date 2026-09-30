import * as fs from 'node:fs/promises'
import { execFileSync, spawn } from 'node:child_process'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { FileHelper } from '../util/fileHelper'
import { z } from '@start9labs/start-core/zExport'
import { FILE_ACCESS_TIMEOUT_MS } from '../util/fileAccess'

jest.mock('node:fs/promises', () => {
  const actual = jest.requireActual('node:fs/promises')
  return {
    ...actual,
    open: jest.fn(actual.open),
    rename: jest.fn(actual.rename),
  }
})
const actual = jest.requireActual('node:fs/promises') as typeof fs
const open = fs.open as jest.MockedFunction<typeof fs.open>
const rename = fs.rename as jest.MockedFunction<typeof fs.rename>
const shape = z.object({
  count: z.number().catch(0),
  a: z.string().optional(),
  b: z.string().optional(),
})
const effects = {} as any
const aclTest = (() => {
  try {
    execFileSync('setfacl', ['--version'], { stdio: 'ignore' })
    execFileSync('getfacl', ['--version'], { stdio: 'ignore' })
    return test
  } catch {
    return test.skip
  }
})()
let dir: string
let path: string
beforeEach(async () => {
  dir = await fs.mkdtemp(join(tmpdir(), 'file-write-'))
  path = join(dir, 'store.json')
})
afterEach(async () => {
  open.mockImplementation(actual.open)
  await fs.rm(dir, { recursive: true, force: true })
})

test('concurrent merges and updates retain every change through aliases', async () => {
  const link = join(dir, 'alias')
  await fs.symlink(path, link)
  const a = FileHelper.json(path, shape)
  const b = FileHelper.json(link, shape)
  await Promise.all([
    a.merge(effects, { a: 'a' }),
    b.merge(effects, { b: 'b' }),
  ])
  await Promise.all(
    Array.from({ length: 20 }, (_, i) =>
      (i % 2 ? a : b).update(effects, async value => {
        await new Promise(resolve => setImmediate(resolve))
        return { ...value!, count: value!.count + 1 }
      }),
    ),
  )
  expect(await a.read().once()).toEqual({ a: 'a', b: 'b', count: 20 })
  expect((await fs.lstat(link)).isSymbolicLink()).toBe(true)
})

test('a failed temp write preserves the target and restricts temp permissions', async () => {
  await fs.writeFile(path, '{"count":1}', { mode: 0o600 })
  open.mockImplementation(async (...args: Parameters<typeof fs.open>) => {
    const handle = await actual.open(...args)
    if (String(args[0]).endsWith('.tmp')) {
      expect((await handle.stat()).mode & 0o777).toBe(0o600)
      handle.writeFile = async data => {
        await handle.write(String(data).slice(0, 4))
        throw new Error('interrupted')
      }
    }
    return handle
  })
  await expect(
    FileHelper.json(path, shape).write(effects, { count: 2 }),
  ).rejects.toThrow('interrupted')
  expect(await fs.readFile(path, 'utf8')).toBe('{"count":1}')
  expect((await fs.readdir(dir)).filter(name => name.endsWith('.tmp'))).toEqual(
    [],
  )
})

test('timed-out filesystem work holds the queue and cannot rename later', async () => {
  await fs.writeFile(path, '{"count":0}')
  jest.useFakeTimers()
  let entered!: () => void
  let finish!: () => void
  const started = new Promise<void>(resolve => (entered = resolve))
  const blocked = new Promise<void>(resolve => (finish = resolve))
  let first = true
  open.mockImplementation(async (...args: Parameters<typeof fs.open>) => {
    const handle = await actual.open(...args)
    if (first && String(args[0]).endsWith('.tmp')) {
      first = false
      const sync = handle.sync.bind(handle)
      handle.sync = async () => {
        entered()
        await blocked
        await sync()
      }
    }
    return handle
  })
  const file = FileHelper.json(path, shape)
  try {
    const writing = file.write(effects, { count: 1 })
    const rejected = expect(writing).rejects.toThrow('File access timed out')
    await started
    await jest.advanceTimersByTimeAsync(FILE_ACCESS_TIMEOUT_MS)
    await rejected
    expect(await fs.readFile(path, 'utf8')).toBe('{"count":0}')
    let recovered = false
    const recovery = file.write(effects, { count: 2 }).then(() => {
      recovered = true
    })
    for (let i = 0; i < 100 && !jest.getTimerCount(); i++) await fs.stat(path)
    expect(recovered).toBe(false)
    finish()
    await recovery
    expect(await file.read().once()).toEqual({ count: 2 })
    expect(
      (await fs.readdir(dir)).filter(name => name.endsWith('.tmp')),
    ).toEqual([])
  } finally {
    finish()
    jest.useRealTimers()
  }
})

test('atomic replacement preserves mode and readers never see partial JSON', async () => {
  await fs.writeFile(path, '{"count":1}', { mode: 0o640 })
  const file = FileHelper.json(path, shape)
  const writing = (async () => {
    for (let i = 2; i <= 5; i++)
      await file.write(effects, { count: i, a: 'x'.repeat(1 << 20) })
  })()
  for (let i = 0; i < 20; i++) expect(await file.read().once()).not.toBeNull()
  await writing
  expect((await fs.stat(path)).mode & 0o777).toBe(0o640)
})

test('update matches once for falsy contents, validates and recovers from callback failure', async () => {
  const file = FileHelper.json(path, shape)
  const skip = jest.fn(() => null)
  await file.update(effects, skip)
  expect(skip).toHaveBeenCalledWith(null)
  await file.write(effects, { count: 1 })
  await expect(
    file.update(effects, () => {
      throw new Error('no')
    }),
  ).rejects.toThrow('no')
  const strict = FileHelper.json(path, z.object({ count: z.number() }))
  await expect(
    strict.update(effects, () => ({ count: 'bad' }) as any),
  ).rejects.toThrow()
  expect(await strict.read().once()).toEqual({ count: 1 })
  await file.update(effects, () => ({ count: 'bad' }) as any)
  expect(await file.read().once()).toEqual({ count: 0 })
  await fs.writeFile(path, '')
  const string = FileHelper.string(path)
  expect(await string.read().once()).toBeNull()
  await string.update(effects, value => {
    expect(value).toBeNull()
    return null
  })
})

test.each([false, true])(
  'nested same-file mutations reject (existing: %s)',
  async existing => {
    const file = FileHelper.json(path, shape)
    const alias = join(dir, 'alias')
    await fs.symlink(path, alias)
    const nested = FileHelper.json(alias, shape)
    if (existing) await file.write(effects, { count: 0 })
    const callback = jest.fn(() => ({ count: 10 }))
    await file.update(effects, async current => {
      expect(await nested.read().once()).toEqual(current)
      for (const mutation of [
        () => nested.write(effects, { count: 10 }),
        () => nested.merge(effects, { count: 10 }),
        () => nested.update(effects, callback),
      ]) {
        await expect(mutation()).rejects.toThrow(
          'Cannot perform a nested mutation',
        )
      }
      expect(callback).not.toHaveBeenCalled()
      expect(await file.read().once()).toEqual(current)
      await FileHelper.string(join(dir, 'other')).write(effects, 'allowed')
      return { count: 1 }
    })
    await expect(
      file.update(effects, () => nested.write(effects, { count: 10 })),
    ).rejects.toThrow('Cannot perform a nested mutation')
    expect(await file.read().once()).toEqual({ count: 1 })
    await file.update(effects, current => ({ count: current!.count + 1 }))
    expect(await file.read().once()).toEqual({ count: 2 })
  },
)

function worker(path: string, mode: string) {
  return spawn(
    process.execPath,
    [
      '--import',
      'tsx',
      join(__dirname, 'fixtures/fileHelper.worker.ts'),
      path,
      mode,
    ],
    { detached: true, stdio: ['ignore', 'pipe', 'pipe'] },
  )
}
function finished(child: ReturnType<typeof worker>) {
  let error = ''
  child.stderr.on('data', chunk => (error += chunk))
  return new Promise<void>((resolve, reject) => {
    child.once('error', reject)
    child.once('close', code =>
      code === 0
        ? resolve()
        : reject(new Error(error || `worker exit ${code}`)),
    )
  })
}

test('separate runtimes serialize the entire read-modify-write', async () => {
  await fs.writeFile(path, '{"count":0}')
  const alias = join(dir, 'alias')
  await fs.symlink(path, alias)
  await Promise.all(
    Array.from({ length: 4 }, (_, i) =>
      finished(worker(i % 2 ? path : alias, '10')),
    ),
  )
  expect(await FileHelper.json(path, shape).read().once()).toEqual({
    count: 40,
  })
}, 20000)

test('separate runtimes serialize creating a missing file', async () => {
  await Promise.all(
    Array.from({ length: 4 }, () => finished(worker(path, '10'))),
  )
  expect(await FileHelper.json(path, shape).read().once()).toEqual({
    count: 40,
  })
  expect(await fs.readdir(dir)).toEqual(['store.json'])
}, 20000)

test('writes wait for another runtime holding the lock', async () => {
  await fs.writeFile(path, '{"count":0}')
  const child = worker(path, 'hold')
  try {
    await new Promise<void>((resolve, reject) => {
      child.stdout.once('data', () => resolve())
      child.once('error', reject)
    })
    let done = false
    const writing = FileHelper.json(path, shape)
      .write(effects, { count: 7 })
      .then(() => (done = true))
    await new Promise(resolve => setTimeout(resolve, 300))
    expect(done).toBe(false)
    child.kill('SIGKILL')
    await writing
    expect(await FileHelper.json(path, shape).read().once()).toEqual({
      count: 7,
    })
  } finally {
    child.kill('SIGKILL')
  }
}, 10000)

test('a killed runtime releases its file lock', async () => {
  await fs.writeFile(path, '{"count":0}')
  const child = worker(path, 'hold')
  try {
    await new Promise<void>((resolve, reject) => {
      child.stdout.once('data', () => resolve())
      child.once('error', reject)
      child.once('exit', () =>
        reject(new Error('worker exited before acquiring lock')),
      )
    })
    const exited = new Promise(resolve => child.once('exit', resolve))
    child.kill('SIGKILL')
    await exited
    await FileHelper.json(path, shape).update(effects, () => ({ count: 1 }))
    expect(await FileHelper.json(path, shape).read().once()).toEqual({
      count: 1,
    })
  } finally {
    child.kill('SIGKILL')
  }
}, 10000)

aclTest('replacement preserves access ACLs', async () => {
  await fs.writeFile(path, 'old', { mode: 0o640 })
  execFileSync('setfacl', ['-m', 'u:65534:---', path])
  const acl = () => execFileSync('getfacl', ['-cp', path], { encoding: 'utf8' })
  const before = acl()
  await FileHelper.string(path).write(effects, 'new')
  expect(acl()).toBe(before)
})

aclTest(
  'new files inherit default ACLs like a normal file creation',
  async () => {
    execFileSync('setfacl', [
      '-m',
      'd:u::rwx,d:g::rwx,d:o::---,d:m::rwx,d:u:65534:rwx',
      dir,
    ])
    const normal = join(dir, 'normal')
    await fs.writeFile(normal, 'normal')
    await FileHelper.string(path).write(effects, 'new')
    const acl = (file: string) =>
      execFileSync('getfacl', ['-cp', file], { encoding: 'utf8' })
    expect(acl(path)).toBe(acl(normal))
    expect((await fs.stat(path)).mode & 0o777).toBe(
      (await fs.stat(normal)).mode & 0o777,
    )
  },
)

test('dangling symlinks retain their identity and create the target directory', async () => {
  await fs.symlink('nested/target', path)
  await FileHelper.string(path).write(effects, 'new')
  expect((await fs.lstat(path)).isSymbolicLink()).toBe(true)
  expect(await fs.readFile(join(dir, 'nested/target'), 'utf8')).toBe('new')
})

test('failure to acquire a lock does not write and does not poison the queue', async () => {
  await fs.chmod(dir, 0o555)
  const file = FileHelper.json(path, shape)
  try {
    await expect(file.merge(effects, { count: 1 })).rejects.toThrow(
      'File lock failed',
    )
  } finally {
    await fs.chmod(dir, 0o755)
  }
  await file.write(effects, { count: 2 })
  expect(await file.read().once()).toEqual({ count: 2 })
})

test('an unreadable target rejects promptly and releases the queue', async () => {
  await fs.writeFile(path, '{"count":0}', { mode: 0o200 })
  const child = worker(path, 'write')
  const done = finished(child)
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    await expect(
      Promise.race([
        done,
        new Promise((_, reject) => {
          timer = setTimeout(
            () => reject(new Error('lock acquisition hung')),
            1000,
          )
        }),
      ]),
    ).rejects.toThrow('File lock failed:')
  } finally {
    clearTimeout(timer)
    if (child.exitCode === null && child.signalCode === null && child.pid) {
      process.kill(-child.pid, 'SIGKILL')
    }
    await done.catch(() => {})
    await fs.chmod(path, 0o600)
  }
  expect(await fs.readFile(path, 'utf8')).toBe('{"count":0}')
  await FileHelper.json(path, shape).write(effects, { count: 1 })
  expect(await FileHelper.json(path, shape).read().once()).toEqual({ count: 1 })
})

test('locking leaves no files behind', async () => {
  const file = FileHelper.json(path, shape)
  await Promise.all(
    Array.from({ length: 5 }, () =>
      file.update(effects, value => ({ count: (value?.count ?? 0) + 1 })),
    ),
  )
  await file.merge(effects, { a: 'a' })
  expect(await file.read().once()).toEqual({ count: 5, a: 'a' })
  await FileHelper.json(join(dir, 'skipped.json'), shape).update(
    effects,
    () => null,
  )
  expect(await fs.readdir(dir)).toEqual(['store.json'])
})

test('a bind-mounted target is written and synced in place', async () => {
  const file = FileHelper.json(path, shape)
  await file.write(effects, { count: 1 })
  const { ino } = await actual.stat(path)
  const sync = jest.fn()
  open.mockImplementation(async (...args: Parameters<typeof fs.open>) => {
    const handle = await actual.open(...args)
    if (args[0] === path) {
      const original = handle.sync.bind(handle)
      handle.sync = async () => {
        sync()
        await original()
      }
    }
    return handle
  })
  rename.mockRejectedValueOnce(
    Object.assign(new Error('EBUSY'), { code: 'EBUSY' }),
  )
  await file.write(effects, { count: 2 })
  expect((await actual.stat(path)).ino).toBe(ino)
  expect(sync).toHaveBeenCalledTimes(1)
  expect(await file.read().once()).toEqual({ count: 2 })
  expect((await actual.readdir(dir)).filter(n => n.endsWith('.tmp'))).toEqual(
    [],
  )
})
