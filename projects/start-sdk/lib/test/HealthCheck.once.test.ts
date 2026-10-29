import { HealthCheck } from '../health/HealthCheck'
import { isInGracePeriod, withGracePeriod } from '../health/isInGracePeriod'
import { cooldownTrigger, onceTrigger } from '../trigger'

const failure = { result: 'failure' as const, message: 'Unavailable' }

describe('HealthCheck finite triggers', () => {
  beforeEach(() => jest.useFakeTimers())
  afterEach(() => jest.useRealTimers())

  it('ends the shared default grace period at its boundary', () => {
    expect(isInGracePeriod(0, 0)).toBe(false)
    expect(isInGracePeriod(0)).toBe(true)
    jest.advanceTimersByTime(9_999)
    expect(isInGracePeriod(0)).toBe(true)
    jest.advanceTimersByTime(1)
    expect(isInGracePeriod(0)).toBe(false)
  })

  it('hides probe errors until the grace period ends', () => {
    expect(withGracePeriod(failure, 0)).toEqual({
      result: 'starting',
      message: null,
    })
    jest.advanceTimersByTime(9_999)
    expect(withGracePeriod(failure, 0)).toEqual({
      result: 'starting',
      message: null,
    })
    jest.advanceTimersByTime(1)
    expect(withGracePeriod(failure, 0)).toBe(failure)
  })

  it.each([
    { result: 'starting', message: 'Preparing database' },
    { result: 'starting', waiting: true, message: 'Database unavailable' },
    { result: 'loading', waiting: true, message: 'Peers unavailable' },
  ] as const)('preserves explicit $result messages during grace', health => {
    expect(withGracePeriod(health, 0)).toBe(health)
  })

  it.each([false, true])(
    'reports a zero-grace failure once (throws: %s)',
    async throws => {
      jest.advanceTimersByTime(1)
      const setHealth = jest.fn(async () => null)
      const fn = jest.fn(() => {
        if (throws) throw new Error(failure.message)
        return failure
      })
      const check = HealthCheck.of({ setHealth } as any, {
        id: 'static',
        name: 'Static',
        gracePeriod: 0,
        trigger: onceTrigger,
        fn,
      })
      try {
        check.start()
        await jest.advanceTimersByTimeAsync(60_000)
        expect(fn).toHaveBeenCalledTimes(1)
        expect(setHealth).toHaveBeenCalledTimes(1)
        expect(setHealth).toHaveBeenLastCalledWith({
          id: 'static',
          name: 'Static',
          ...failure,
        })
      } finally {
        check.onDrop()
      }
    },
  )

  it.each([
    { result: 'starting', waiting: true, message: 'Waiting: database <ready>' },
    { result: 'loading', waiting: true, message: 'Sync paused: no peers' },
    { result: 'starting', message: 'Starting: custom details' },
    { result: 'loading', message: 'Syncing: 25%' },
    { result: 'success', message: '' },
    { result: 'disabled', message: 'Disabled: custom details' },
    { result: 'failure', message: 'Failure: custom details' },
  ] as const)('preserves the $result payload', async health => {
    const setHealth = jest.fn(async () => null)
    const check = HealthCheck.of({ setHealth } as any, {
      id: 'static',
      name: 'Static',
      gracePeriod: 0,
      trigger: onceTrigger,
      fn: () => health,
    })
    try {
      check.start()
      await jest.advanceTimersByTimeAsync(0)
      expect(setHealth).toHaveBeenLastCalledWith({
        id: 'static',
        name: 'Static',
        ...health,
      })
    } finally {
      check.onDrop()
    }
  })

  it('creates a new trigger after stop/start, including in the same clock tick', async () => {
    jest.advanceTimersByTime(1)
    const fn = jest.fn(() => failure)
    const check = HealthCheck.of({ setHealth: async () => null } as any, {
      id: 'static',
      name: 'Static',
      gracePeriod: 0,
      trigger: onceTrigger,
      fn,
    })
    try {
      check.start()
      await jest.advanceTimersByTimeAsync(0)
      expect(fn).toHaveBeenCalledTimes(1)
      check.stop()
      check.start()
      await jest.advanceTimersByTimeAsync(0)
      expect(fn).toHaveBeenCalledTimes(2)
      check.onDrop()
      await jest.advanceTimersByTimeAsync(60_000)
      expect(fn).toHaveBeenCalledTimes(2)
    } finally {
      check.onDrop()
    }
  })

  it('retains the trigger delay after stop/start', async () => {
    jest.advanceTimersByTime(1)
    const fn = jest.fn(() => failure)
    const check = HealthCheck.of({ setHealth: async () => null } as any, {
      id: 'periodic',
      name: 'Periodic',
      gracePeriod: 0,
      trigger: cooldownTrigger(100),
      fn,
    })
    try {
      check.start()
      await jest.advanceTimersByTimeAsync(100)
      expect(fn).toHaveBeenCalledTimes(1)
      check.stop()
      check.start()
      await jest.advanceTimersByTimeAsync(99)
      expect(fn).toHaveBeenCalledTimes(1)
      await jest.advanceTimersByTimeAsync(1)
      expect(fn).toHaveBeenCalledTimes(2)
    } finally {
      check.onDrop()
    }
  })
})
