import { cooldownTrigger, statusTrigger } from '../trigger'

const MAX_TIMEOUT_MS = 2 ** 31 - 1

describe('trigger delays past the timer limit', () => {
  beforeEach(() => jest.useFakeTimers())
  afterEach(() => jest.useRealTimers())

  const yieldsBy = async (next: Promise<unknown>, ms: number) => {
    let yielded = false
    next.then(() => (yielded = true))
    await jest.advanceTimersByTimeAsync(ms)
    return yielded
  }

  it('never polls again after an infinite cooldown', async () => {
    const trigger = cooldownTrigger(Infinity)(() => ({}))
    expect(await yieldsBy(trigger.next(), MAX_TIMEOUT_MS)).toBe(false)
  })

  it('waits out a cooldown longer than one timer can hold', async () => {
    const trigger = cooldownTrigger(MAX_TIMEOUT_MS + 1_000)(() => ({}))
    const next = trigger.next()
    expect(await yieldsBy(next, MAX_TIMEOUT_MS)).toBe(false)
    expect(await yieldsBy(next, 1_000)).toBe(true)
  })

  it('never polls again from an infinite status interval', async () => {
    const trigger = statusTrigger(Infinity, { starting: 1_000 })(() => ({
      lastResult: 'failure',
    }))
    expect(await yieldsBy(trigger.next(), MAX_TIMEOUT_MS)).toBe(false)
  })
})
