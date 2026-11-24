import { delay, ExponentialBackoff } from '../util/delay'

const MAX_TIMEOUT_MS = 2 ** 31 - 1

describe('delay', () => {
  beforeEach(() => jest.useFakeTimers())
  afterEach(() => jest.useRealTimers())

  const settledBy = async (pending: Promise<void>, ms: number) => {
    let settled = false
    pending.then(() => (settled = true))
    await jest.advanceTimersByTimeAsync(ms)
    return settled
  }

  it('resolves at once when already aborted', async () => {
    const ctrl = new AbortController()
    ctrl.abort()
    expect(await settledBy(delay(Infinity, ctrl.signal), 0)).toBe(true)
    expect(jest.getTimerCount()).toBe(0)
  })

  it('resolves on abort and clears its timer', async () => {
    const ctrl = new AbortController()
    const pending = delay(MAX_TIMEOUT_MS + 1_000, ctrl.signal)
    expect(await settledBy(pending, MAX_TIMEOUT_MS)).toBe(false)
    ctrl.abort()
    expect(await settledBy(pending, 0)).toBe(true)
    expect(jest.getTimerCount()).toBe(0)
  })

  it('resolves an infinite delay on abort', async () => {
    const ctrl = new AbortController()
    const pending = delay(Infinity, ctrl.signal)
    expect(await settledBy(pending, MAX_TIMEOUT_MS)).toBe(false)
    ctrl.abort()
    expect(await settledBy(pending, 0)).toBe(true)
  })
})

describe('ExponentialBackoff', () => {
  it('doubles up to the cap and restarts after reset', () => {
    const backoff = new ExponentialBackoff(100, 500)
    expect([1, 2, 3, 4, 5].map(() => backoff.next())).toEqual([
      100, 200, 400, 500, 500,
    ])
    backoff.reset()
    expect(backoff.next()).toBe(100)
  })
})
