import * as fs from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { FileMounts, hasFileMounts } from '../util/fileMounts'
import { FileHelper } from '../util/fileHelper'

let dir: string
let source: string
let target: string
let mounts: FileMounts
let rebind: jest.Mock<Promise<void>, []>
async function relink(from: string, to: string) {
  const temp = `${to}.next`
  await fs.link(from, temp)
  await fs.rename(temp, to)
}
beforeEach(async () => {
  dir = await fs.mkdtemp(join(tmpdir(), 'file-mount-'))
  source = join(dir, 'source')
  target = join(dir, 'target')
  await fs.writeFile(source, 'old')
  await fs.link(source, target)
  mounts = new FileMounts()
  rebind = jest.fn(() => relink(source, target))
  await mounts.add(source, target, rebind)
})
afterEach(async () => {
  await mounts.close()
  await fs.rm(dir, { recursive: true, force: true })
})

test('writes await refresh and repeated replacements update every mounted view', async () => {
  const second = join(dir, 'second')
  await fs.link(source, second)
  await mounts.add(source, second, () => relink(source, second))
  const file = FileHelper.string(source)
  for (const value of ['one', 'two', 'three']) {
    await file.write({} as any, value)
    expect(await fs.readFile(target, 'utf8')).toBe(value)
    expect(await fs.readFile(second, 'utf8')).toBe(value)
  }
  expect(rebind).toHaveBeenCalledTimes(3)
})

test('the directory watch follows external source replacements', async () => {
  const temp = join(dir, 'temp')
  await fs.writeFile(temp, 'external')
  await fs.rename(temp, source)
  for (
    let i = 0;
    i < 100 && (await fs.readFile(target, 'utf8')) !== 'external';
    i++
  ) {
    await new Promise(resolve => setTimeout(resolve, 10))
  }
  expect(await fs.readFile(target, 'utf8')).toBe('external')
})

test('reads reconcile before emitting a changed const', async () => {
  const context = {
    isInContext: true,
    onLeaveContext: () => {},
    constRetry: jest.fn(),
  } as any
  const file = FileHelper.string(source)
  expect(await file.read().const(context)).toBe('old')
  const temp = join(dir, 'temp')
  await fs.writeFile(temp, 'new')
  await fs.rename(temp, source)
  expect(await file.read().once()).toBe('new')
  expect(await fs.readFile(target, 'utf8')).toBe('new')
  for (let i = 0; i < 100 && !context.constRetry.mock.calls.length; i++) {
    await new Promise(resolve => setTimeout(resolve, 10))
  }
  expect(context.constRetry).toHaveBeenCalled()
  context.isInContext = false
})

test('refresh failures block commands until a retry succeeds', async () => {
  let fail = true
  const errors = jest.spyOn(console, 'error').mockImplementation(() => {})
  rebind.mockImplementation(async () => {
    if (fail) throw new Error('rebind failed')
    await relink(source, target)
  })
  try {
    const file = FileHelper.string(source)
    await expect(file.write({} as any, 'new')).rejects.toThrow('rebind failed')
    await expect(file.read().once()).rejects.toThrow('rebind failed')
    expect(await fs.readFile(source, 'utf8')).toBe('new')
    await expect(mounts.sync()).rejects.toThrow('rebind failed')
    expect(await fs.readFile(target, 'utf8')).toBe('old')
    fail = false
    await mounts.sync()
    expect(await fs.readFile(target, 'utf8')).toBe('new')
  } finally {
    errors.mockRestore()
  }
})

test('a deleted source leaves the mount on its last file', async () => {
  await fs.rm(source)
  await mounts.sync()
  expect(rebind).not.toHaveBeenCalled()
  expect(await fs.readFile(target, 'utf8')).toBe('old')
})

test('reads and mount sync inside update reenter the queue', async () => {
  const file = FileHelper.string(source)
  await file.update({} as any, async current => {
    expect(await file.read().once()).toBe(current)
    await mounts.sync()
    return `${current}-outer`
  })
  expect(await file.read().once()).toBe('old-outer')
  expect(await fs.readFile(target, 'utf8')).toBe('old-outer')
})

test('mount reconciliation gives up when rebinding makes no progress', async () => {
  const errors = jest.spyOn(console, 'error').mockImplementation(() => {})
  rebind.mockImplementation(async () => {})
  try {
    const file = FileHelper.string(source)
    await expect(file.write({} as any, 'new')).rejects.toThrow(
      'does not follow',
    )
    rebind.mockImplementation(() => relink(source, target))
    await mounts.sync()
    expect(await fs.readFile(target, 'utf8')).toBe('new')
  } finally {
    errors.mockRestore()
  }
})

test('teardown unregisters watches and sync skips unchanged mounts', async () => {
  await mounts.sync()
  expect(rebind).not.toHaveBeenCalled()
  expect(hasFileMounts(source)).toBe(true)
  await mounts.close()
  expect(hasFileMounts(source)).toBe(false)
  await FileHelper.string(source).write({} as any, 'new')
  await mounts.sync()
  expect(rebind).not.toHaveBeenCalled()
  expect(await fs.readFile(target, 'utf8')).toBe('old')
})

test('teardown waits for in-flight refreshes and queued watch callbacks', async () => {
  let enter!: () => void
  let finish!: () => void
  const entered = new Promise<void>(resolve => {
    enter = resolve
  })
  const blocked = new Promise<void>(resolve => {
    finish = resolve
  })
  rebind.mockImplementation(async () => {
    enter()
    await blocked
    await relink(source, target)
  })
  const temp = join(dir, 'temp')
  await fs.writeFile(temp, 'new')
  await fs.rename(temp, source)
  const syncing = mounts.sync()
  await entered
  let closed = false
  const closing = mounts.close().then(() => {
    closed = true
  })
  await new Promise(resolve => setTimeout(resolve, 10))
  expect(closed).toBe(false)
  finish()
  await Promise.all([syncing, closing])
  expect(rebind).toHaveBeenCalledTimes(1)
  expect(hasFileMounts(source)).toBe(false)
})
