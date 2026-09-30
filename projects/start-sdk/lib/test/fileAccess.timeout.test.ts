import * as fs from 'node:fs/promises'
import { spawn } from 'node:child_process'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { FileHelper } from '../util/fileHelper'
import { UPDATE_TIMEOUT_MS, withFileQueue } from '../util/fileAccess'

let dir: string
let path: string
beforeEach(async () => {
  dir = await fs.mkdtemp(join(tmpdir(), 'file-timeout-'))
  path = join(dir, 'store')
  await fs.writeFile(path, 'old')
  jest.useFakeTimers()
})
afterEach(async () => {
  jest.useRealTimers()
  await fs.rm(dir, { recursive: true, force: true })
})

function gate() {
  let resolve!: () => void
  const promise = new Promise<void>(done => (resolve = done))
  return { promise, resolve }
}

async function waitForTimers(count: number) {
  for (let i = 0; i < 100 && jest.getTimerCount() < count; i++)
    await fs.stat(path)
  expect(jest.getTimerCount()).toBeGreaterThanOrEqual(count)
}

test('a timed-out update releases its lock and cannot commit later', async () => {
  const entered = gate()
  const finish = gate()
  const continued = gate()
  const file = FileHelper.string(path)
  const updating = file.update({} as any, async () => {
    entered.resolve()
    await finish.promise
    await expect(file.write({} as any, 'late')).rejects.toThrow(
      'File update timed out',
    )
    continued.resolve()
    return 'late return'
  })
  const rejected = expect(updating).rejects.toThrow('File update timed out')
  await entered.promise
  await jest.advanceTimersByTimeAsync(UPDATE_TIMEOUT_MS)
  await rejected
  await file.write({} as any, 'recovered')
  finish.resolve()
  await continued.promise
  expect(await file.read().once()).toBe('recovered')
})

test('queued access waits for its holder without a deadline', async () => {
  const entered = gate()
  const finish = gate()
  const holding = withFileQueue(path, async () => {
    entered.resolve()
    await finish.promise
  })
  await entered.promise
  const callback = jest.fn(async () => {})
  const queued = withFileQueue(path, callback)
  await jest.advanceTimersByTimeAsync(UPDATE_TIMEOUT_MS * 10)
  expect(callback).not.toHaveBeenCalled()
  finish.resolve()
  await Promise.all([holding, queued])
  expect(callback).toHaveBeenCalledTimes(1)
})

test('a cross-file update cycle times out and both queues recover', async () => {
  const other = join(dir, 'other')
  await fs.writeFile(other, 'other old')
  const a = FileHelper.string(path)
  const b = FileHelper.string(other)
  const aEntered = gate()
  const bEntered = gate()
  const updateA = a.update({} as any, async () => {
    aEntered.resolve()
    await bEntered.promise
    await b.update({} as any, () => 'nested B')
    return 'new A'
  })
  const updateB = b.update({} as any, async () => {
    bEntered.resolve()
    await aEntered.promise
    await a.update({} as any, () => 'nested A')
    return 'new B'
  })
  const rejectedA = expect(updateA).rejects.toThrow('File update timed out')
  const rejectedB = expect(updateB).rejects.toThrow('File update timed out')
  await waitForTimers(2)
  await jest.advanceTimersByTimeAsync(UPDATE_TIMEOUT_MS)
  await Promise.all([rejectedA, rejectedB])
  expect(await a.read().once()).toBe('old')
  expect(await b.read().once()).toBe('other old')
  await Promise.all([
    a.write({} as any, 'recovered A'),
    b.write({} as any, 'recovered B'),
  ])
  expect(await a.read().once()).toBe('recovered A')
  expect(await b.read().once()).toBe('recovered B')
})

test('an update deadline aborts nested lock acquisition without leaving a child holding the lock', async () => {
  const other = join(dir, 'other')
  await fs.writeFile(other, 'other old')
  const blocker = spawn(
    'flock',
    ['--exclusive', other, 'sh', '-c', 'echo locked; cat >/dev/null'],
    {
      detached: true,
      stdio: ['pipe', 'pipe', 'pipe'],
    },
  )
  const exited = new Promise<void>(resolve =>
    blocker.once('close', () => resolve()),
  )
  try {
    await new Promise<void>((resolve, reject) => {
      blocker.stdout.once('data', () => resolve())
      blocker.once('error', reject)
    })
    const a = FileHelper.string(path)
    const b = FileHelper.string(other)
    const updating = a.update({} as any, async () => {
      await b.write({} as any, 'nested')
      return 'new'
    })
    const rejected = expect(updating).rejects.toThrow('File update timed out')
    await waitForTimers(1)
    await jest.advanceTimersByTimeAsync(UPDATE_TIMEOUT_MS)
    await rejected
    blocker.stdin.end()
    await exited
    await b.write({} as any, 'recovered')
    expect(await a.read().once()).toBe('old')
    expect(await b.read().once()).toBe('recovered')
  } finally {
    if (blocker.exitCode === null && blocker.signalCode === null && blocker.pid)
      process.kill(-blocker.pid, 'SIGKILL')
    await exited
  }
})
