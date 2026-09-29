import {
  chmodSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from 'fs'
import * as fsPromises from 'node:fs/promises'
import { tmpdir } from 'os'
import { join } from 'path'
import { FileHelper, isBindMountSource } from '../util/fileHelper'
import { z } from '@start9labs/start-core/zExport'

jest.mock('node:fs/promises', () => {
  const actual = jest.requireActual('node:fs/promises')
  return { ...actual, writeFile: jest.fn(actual.writeFile) }
})

const actualFs = jest.requireActual(
  'node:fs/promises',
) as typeof import('node:fs/promises')
const writeFile = fsPromises.writeFile as jest.MockedFunction<
  typeof fsPromises.writeFile
>

const shape = z.looseObject({
  A: z.string().catch('a'),
  B: z.string().optional(),
  C: z.string().optional(),
  count: z.number().catch(0),
})
const effects = {} as any

const dir = mkdtempSync(join(tmpdir(), 'file-helper-write-'))
let n = 0
const seeded = (contents: string | null) => {
  const path = join(dir, `${n++}.json`)
  if (contents !== null) writeFileSync(path, contents)
  return path
}
const siblings = (path: string) => {
  const name = path.slice(dir.length + 1)
  return readdirSync(dir).filter(f => f !== name && f.includes(name))
}

describe('FileHelper writes', () => {
  // A process killed mid-write used to leave the file truncated, and every
  // later read threw on it.
  test('a write that dies partway leaves the previous contents', async () => {
    const before = '{"A":"old","count":1}'
    const path = seeded(before)
    const file = FileHelper.json(path, shape)
    writeFile.mockImplementationOnce(async (target, data) => {
      await actualFs.writeFile(target, String(data).slice(0, 4))
      throw new Error('killed')
    })

    await expect(file.write(effects, { A: 'new', count: 2 })).rejects.toThrow(
      'killed',
    )

    expect(readFileSync(path, 'utf-8')).toBe(before)
    expect(await file.read().once()).toEqual({ A: 'old', count: 1 })
    expect(siblings(path)).toEqual([])
  })

  test('a reader never sees a partly written file', async () => {
    const big = (fill: string) =>
      ({ A: fill.repeat(4 << 20), count: 0 }) as z.infer<typeof shape>
    const path = seeded(JSON.stringify(big('x')))
    const file = FileHelper.json(path, shape)
    let writing = true
    const reads: string[] = []
    const reader = (async () => {
      while (writing) {
        reads.push(await actualFs.readFile(path, 'utf-8'))
      }
    })()

    for (const fill of ['y', 'z', 'w']) {
      await file.write(effects, big(fill))
    }
    writing = false
    await reader

    expect(reads.length).toBeGreaterThan(0)
    for (const read of reads) expect(() => JSON.parse(read)).not.toThrow()
  })

  test('concurrent merges keep every key', async () => {
    const path = seeded('{"A":"keep"}')

    await Promise.all([
      FileHelper.json(path, shape).merge(effects, { B: 'b' }),
      FileHelper.json(path, shape).merge(effects, { C: 'c' }),
    ])

    expect(JSON.parse(readFileSync(path, 'utf-8'))).toEqual({
      A: 'keep',
      B: 'b',
      C: 'c',
      count: 0,
    })
  })

  test('a write keeps the permissions of the file it replaces', async () => {
    const path = seeded('{"A":"old"}')
    chmodSync(path, 0o640)

    await FileHelper.json(path, shape).write(effects, { A: 'new', count: 0 })

    expect(statSync(path).mode & 0o777).toBe(0o640)
    expect(JSON.parse(readFileSync(path, 'utf-8')).A).toBe('new')
  })

  test('a write through a symlink replaces the target and keeps the link', async () => {
    const target = seeded('{"A":"old"}')
    const link = join(dir, `${n++}-link.json`)
    symlinkSync(target, link)

    await FileHelper.json(link, shape).write(effects, { A: 'new', count: 0 })

    expect(readdirSync(dir)).toContain(link.slice(dir.length + 1))
    expect(JSON.parse(readFileSync(target, 'utf-8')).A).toBe('new')
    expect(statSync(link).ino).toBe(statSync(target).ino)
  })

  test('a watcher sees a replaced file', async () => {
    const path = seeded('{"A":"old"}')
    const file = FileHelper.json(path, shape)
    const context = {
      isInContext: true,
      onLeaveContext: () => {},
    } as any
    const abort = new AbortController()
    const watch = file.read(s => s.A).watch(context, abort.signal)

    expect((await watch.next()).value).toBe('old')
    const next = watch.next()
    await file.write(effects, { A: 'new', count: 0 })

    expect((await next).value).toBe('new')
    abort.abort()
    context.isInContext = false
  })
})

describe('FileHelper.update', () => {
  test('passes the current value and writes what the change returns', async () => {
    const path = seeded('{"A":"keep","count":1,"extra":true}')
    const file = FileHelper.json(path, shape)
    const change = jest.fn((current: z.infer<typeof shape> | null) => ({
      ...current!,
      count: current!.count + 1,
    }))

    await file.update(effects, change)

    expect(change).toHaveBeenCalledWith({ A: 'keep', count: 1, extra: true })
    expect(JSON.parse(readFileSync(path, 'utf-8'))).toEqual({
      A: 'keep',
      count: 2,
      extra: true,
    })
  })

  test('runs concurrent changes one after another', async () => {
    const path = seeded('{"A":"a","count":0}')
    const file = FileHelper.json(path, shape)

    await Promise.all(
      Array.from({ length: 25 }, () =>
        file.update(effects, async current => {
          await new Promise(resolve => setImmediate(resolve))
          return { ...current!, count: current!.count + 1 }
        }),
      ),
    )

    expect(JSON.parse(readFileSync(path, 'utf-8')).count).toBe(25)
  })

  test('passes null for a missing file, and null leaves it missing', async () => {
    const path = seeded(null)
    const file = FileHelper.json(path, shape)
    const change = jest.fn(() => null)

    await file.update(effects, change)

    expect(change).toHaveBeenCalledWith(null)
    expect(await file.read().once()).toBeNull()
  })

  test('validates what the change returns', async () => {
    const path = seeded('{"A":"a","count":3}')
    const file = FileHelper.json(path, shape)

    await file.update(effects, () => ({ A: 'b', count: 'nope' }) as any)

    expect(JSON.parse(readFileSync(path, 'utf-8'))).toEqual({
      A: 'b',
      count: 0,
    })
  })

  test('a failing change leaves the file and the queue usable', async () => {
    const before = '{"A":"a","count":3}'
    const path = seeded(before)
    const file = FileHelper.json(path, shape)

    await expect(
      file.update(effects, () => {
        throw new Error('no')
      }),
    ).rejects.toThrow('no')
    expect(readFileSync(path, 'utf-8')).toBe(before)

    await file.merge(effects, { count: 4 })
    expect(JSON.parse(readFileSync(path, 'utf-8')).count).toBe(4)
  })

  test('throws on a file it cannot parse, like read().once()', async () => {
    const path = seeded('{"A":')
    const file = FileHelper.json(path, shape)
    const change = jest.fn(() => null)

    await expect(file.update(effects, change)).rejects.toThrow()
    expect(change).not.toHaveBeenCalled()
  })

  test('rejects a write that changes a const-read value', async () => {
    const path = seeded('{"A":"a","count":0}')
    const file = FileHelper.json(path, shape)
    const constRetry = jest.fn()
    const context = {
      isInContext: true,
      onLeaveContext: () => {},
      constRetry,
    } as any

    expect(await file.read(s => s.A).const(context)).toBe('a')

    await expect(
      file.update(context, current => ({ ...current!, A: 'b' })),
    ).rejects.toThrow('write after const')
    await file.update(context, current => ({ ...current!, count: 1 }))
    context.isInContext = false
  })
})

describe('isBindMountSource', () => {
  const mountinfo = [
    '22 1 0:41 / / rw,relatime - ext4 /dev/sda1 rw',
    '30 22 0:41 /srv/volumes/main /media/startos/volumes/main rw - ext4 /dev/sda1 rw',
    '31 22 0:41 /srv/volumes/main/config.toml /media/startos/sub/rootfs/etc/app/config.toml rw - ext4 /dev/sda1 rw',
    '32 22 0:41 /srv/volumes/main/with\\040space /media/startos/sub/rootfs/etc/space rw - ext4 /dev/sda1 rw',
    '33 22 0:52 /srv/volumes/main/other.toml /elsewhere rw - xfs /dev/sdb1 rw',
  ].join('\n')

  test.each([
    ['/media/startos/volumes/main/config.toml', '0:41', true],
    ['/srv/volumes/main/config.toml', '0:41', true],
    ['/media/startos/volumes/main/with space', '0:41', true],
    ['/media/startos/volumes/main/store.json', '0:41', false],
    ['/media/startos/volumes/main/other.toml', '0:41', false],
    ['/media/startos/volumes/main/config.toml', '0:99', false],
  ])('%s on %s → %s', (path, device, expected) => {
    expect(isBindMountSource(mountinfo, path, device)).toBe(expected)
  })
})
