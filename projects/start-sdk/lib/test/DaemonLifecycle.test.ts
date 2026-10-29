import { getEventListeners } from 'node:events'
import { setFlagsFromString } from 'node:v8'
import { runInNewContext } from 'node:vm'
import { Daemon } from '../mainFn/Daemon'
import { EXIT_SUCCESS, HealthDaemon } from '../mainFn/HealthDaemon'
import { HealthCheckResult } from '../health/checkFns'
import { cooldownTrigger, onceTrigger, Trigger } from '../trigger'
import * as T from '@start9labs/start-core/types'

type Manifest = {
  id: 'test'
  volumes: ['main']
  images: { main: { source: { dockerTag: 'x' } } }
} & T.SDKManifest

/** Minimal mock effects sufficient to construct/run daemons in unit tests. */
const fakeEffects = (): T.Effects =>
  ({
    eventId: null,
    child: () => fakeEffects(),
    isInContext: true,
    onLeaveContext: () => {},
    setHealth: async () => null,
    subcontainer: {
      createFs: async () => ['', ''] as any,
      destroyFs: async () => null,
    },
  }) as any

const tick = () => new Promise<void>(resolve => setTimeout(resolve, 0))

/**
 * A stand-in for `SubContainerEager` that models exactly the hold/destroy
 * contract the daemon relies on: `destroyFs` fires when `destroy()` has been
 * called *and* the last hold is released, and `hold()` throws once destroyed.
 */
class FakeSub {
  identity = Symbol('fake-sub')
  holdCount = 0
  destroyed = false
  destroyRequested = false
  destroyFsCount = 0
  detached = false

  detach() {
    this.detached = true
  }
  hold(): () => Promise<void> {
    if (this.destroyed) {
      throw new Error(
        `cannot hold subcontainer ${String(this.identity.description)}: already destroyed`,
      )
    }
    this.holdCount++
    let released = false
    return async () => {
      if (released) return
      released = true
      this.holdCount--
      if (this.holdCount === 0 && this.destroyRequested) await this._destroy()
    }
  }
  async destroy(): Promise<void> {
    this.destroyRequested = true
    if (this.holdCount === 0) await this._destroy()
  }
  private async _destroy(): Promise<void> {
    if (this.destroyed) return
    this.destroyed = true
    this.destroyFsCount++
  }
}

// A JS-only exec that stays running until its CommandController aborts it, so
// the daemon's run loop parks in `wait()` instead of hot-restarting.
const blockUntilAborted = {
  fn: (_sub: unknown, signal: AbortSignal) =>
    new Promise<null>(resolve => {
      if (signal.aborted) return resolve(null)
      signal.addEventListener('abort', () => resolve(null), { once: true })
    }),
}

describe('Daemon.stop vs Daemon.term (subcontainer lifecycle)', () => {
  it('stop() releases the hold but does NOT destroy the subcontainer, and a later start() re-holds it', async () => {
    const effects = fakeEffects()
    const sub = new FakeSub()
    const daemon = Daemon.of<Manifest>()(
      effects,
      sub as any,
      blockUntilAborted as any,
    )

    await daemon.start()
    await tick()
    expect(sub.holdCount).toBe(1)

    await daemon.stop()
    // Hold released, but no destroy — the container survives the pause.
    expect(sub.holdCount).toBe(0)
    expect(sub.destroyed).toBe(false)
    expect(sub.destroyFsCount).toBe(0)

    // The regression: this second start() used to throw `already destroyed`.
    await daemon.start()
    await tick()
    expect(sub.holdCount).toBe(1)
    expect(sub.destroyed).toBe(false)

    await daemon.stop()
    expect(sub.holdCount).toBe(0)
    expect(sub.destroyed).toBe(false)
  })

  it('term() stops the daemon AND destroys the subcontainer (one-way teardown)', async () => {
    const effects = fakeEffects()
    const sub = new FakeSub()
    const daemon = Daemon.of<Manifest>()(
      effects,
      sub as any,
      blockUntilAborted as any,
    )

    await daemon.start()
    await tick()
    expect(sub.holdCount).toBe(1)

    await daemon.term()
    expect(sub.holdCount).toBe(0)
    expect(sub.destroyed).toBe(true)
    expect(sub.destroyFsCount).toBe(1)
  })

  it('stop() then term() still fires destroyFs exactly once (no leak, no double-destroy)', async () => {
    const effects = fakeEffects()
    const sub = new FakeSub()
    const daemon = Daemon.of<Manifest>()(
      effects,
      sub as any,
      blockUntilAborted as any,
    )

    await daemon.start()
    await tick()
    await daemon.stop()
    expect(sub.destroyFsCount).toBe(0)

    await daemon.term()
    expect(sub.destroyed).toBe(true)
    expect(sub.destroyFsCount).toBe(1)

    // term() is idempotent — a second term() must not double-destroy.
    await daemon.term()
    expect(sub.destroyFsCount).toBe(1)
  })
})

describe('Daemon restart loop', () => {
  beforeEach(() => jest.useFakeTimers())
  afterEach(() => jest.useRealTimers())

  it('holds one abort listener across retries of a failing command', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const daemon = Daemon.of<Manifest>()(fakeEffects(), null, {
      fn: async () => {
        throw new Error('boom')
      },
    })
    await daemon.start()
    const signal: AbortSignal = (daemon as any).loop.abort.signal

    const listeners: number[] = []
    for (let i = 0; i < 4; i++) {
      await jest.advanceTimersByTimeAsync(30_000)
      listeners.push(getEventListeners(signal, 'abort').length)
    }
    expect(daemon.failedExits).toBeGreaterThan(11)
    expect(listeners).toEqual([1, 1, 1, 1])

    // Under fake timers the pending backoff only ends through the abort.
    await daemon.term()
    expect(getEventListeners(signal, 'abort')).toHaveLength(0)
    logged.mockRestore()
  })
})

describe('HealthDaemon dependency-driven restart cycle', () => {
  // A fake dependency exposing just the surface HealthDaemon.updateStatus reads.
  const fakeDep = () => ({
    id: 'dep',
    running: true,
    _health: { result: 'success' as const, message: null },
    ready: { display: 'Dep' },
    addWatcher: () => {},
  })

  // A fake managed daemon that records which lifecycle method HealthDaemon calls.
  const fakeDaemon = () => ({
    start: jest.fn(async () => {}),
    stop: jest.fn(async () => {}),
    term: jest.fn(async () => {}),
    onExit: jest.fn(),
    isOneshot: () => false,
  })

  it('pauses with stop() (not term()) when a dependency flaps, then resumes with start()', async () => {
    const effects = fakeEffects()
    const daemon = fakeDaemon()
    const dep = fakeDep()
    const hd = new HealthDaemon<Manifest>(
      daemon as any,
      [dep] as any,
      'dependent',
      EXIT_SUCCESS,
      effects,
    )

    // Dependency ready → daemon launches.
    await hd.updateStatus()
    expect(daemon.start).toHaveBeenCalledTimes(1)
    expect(daemon.stop).not.toHaveBeenCalled()
    expect(daemon.term).not.toHaveBeenCalled()

    // Dependency ready-flaps to not-ready → this is a PAUSE, so the daemon is
    // stopped without destroying its subcontainer. Calling term() here (the
    // pre-fix behavior) is the regression, so guard against it explicitly.
    dep.running = false
    await hd.updateStatus()
    expect(daemon.stop).toHaveBeenCalledTimes(1)
    expect(daemon.term).not.toHaveBeenCalled()

    // Dependency recovers → the same daemon/subcontainer is restarted.
    dep.running = true
    await hd.updateStatus()
    expect(daemon.start).toHaveBeenCalledTimes(2)
    expect(daemon.term).not.toHaveBeenCalled()

    // Only a genuine teardown destroys the subcontainer.
    await hd.term()
    expect(daemon.term).toHaveBeenCalledTimes(1)
  })

  it('surfaces a failed start() as a health failure instead of an unhandled rejection', async () => {
    const effects = fakeEffects()
    const daemon = fakeDaemon()
    daemon.start.mockRejectedValueOnce(
      new Error('cannot hold subcontainer sub: already destroyed'),
    )
    const dep = fakeDep()
    const hd = new HealthDaemon<Manifest>(
      daemon as any,
      [dep] as any,
      'dependent',
      EXIT_SUCCESS,
      effects,
    )

    // updateStatus must resolve (not reject) even though start() threw.
    await expect(hd.updateStatus()).resolves.toBeUndefined()
    expect(hd.health.result).toBe('failure')

    await hd.term()
  })

  it('serializes transitions so a fast flap (ready before stop() drains) resumes cleanly', async () => {
    const effects = fakeEffects()
    // stop() parks until released, modelling a process that takes time to drain.
    const order: string[] = []
    let releaseStop: () => void = () => {}
    const daemon = {
      start: jest.fn(async () => {
        order.push('start')
      }),
      stop: jest.fn(async () => {
        order.push('stop:begin')
        await new Promise<void>(resolve => (releaseStop = resolve))
        order.push('stop:end')
      }),
      term: jest.fn(async () => {}),
      onExit: jest.fn(),
      isOneshot: () => false,
    }
    const dep = fakeDep()
    const hd = new HealthDaemon<Manifest>(
      daemon as any,
      [dep] as any,
      'dependent',
      EXIT_SUCCESS,
      effects,
    )

    // Dependency ready → daemon launches.
    await hd.updateStatus()
    expect(daemon.start).toHaveBeenCalledTimes(1)

    // Dependency flaps not-ready: the pause's stop() begins but parks.
    dep.running = false
    const pausing = hd.updateStatus()
    await tick()
    expect(order).toEqual(['start', 'stop:begin'])

    // Dependency flaps back to ready BEFORE stop() finishes. The resume must wait
    // for the in-flight pause — starting now would no-op against the still-running
    // loop and strand the daemon stopped-but-believed-running. Without
    // serialization start() would be called a second time here.
    dep.running = true
    const resuming = hd.updateStatus()
    await tick()
    expect(daemon.start).toHaveBeenCalledTimes(1)
    expect(order).toEqual(['start', 'stop:begin'])

    // Let the pause finish; the queued resume then restarts — strictly in order.
    releaseStop()
    await pausing
    await resuming
    expect(order).toEqual(['start', 'stop:begin', 'stop:end', 'start'])
    expect(daemon.start).toHaveBeenCalledTimes(2)
    expect(daemon.term).not.toHaveBeenCalled()
  })
})

describe('HealthDaemon.term', () => {
  it('does not start the daemon from a status update still in flight', async () => {
    let releaseSetHealth = () => {}
    const effects = {
      ...fakeEffects(),
      setHealth: () =>
        new Promise<null>(resolve => (releaseSetHealth = () => resolve(null))),
    } as any
    const sub = new FakeSub()
    const releaseSibling = sub.hold()
    const daemon = Daemon.of<Manifest>()(
      effects,
      sub as any,
      blockUntilAborted as any,
    )
    const hd = new HealthDaemon<Manifest>(
      daemon,
      [],
      'db',
      {
        display: 'Database',
        fn: () => ({ result: 'success', message: null }),
      },
      effects,
    )

    const update = hd.updateStatus()
    await tick()
    await hd.term()
    releaseSetHealth()
    await update
    await tick()

    expect(sub.holdCount).toBe(1)
    await releaseSibling()
    expect(sub.destroyed).toBe(true)
  })
})

describe('HealthDaemon ready polling', () => {
  const fakeDaemon = () => ({
    start: jest.fn(async () => {}),
    stop: jest.fn(async () => {}),
    term: jest.fn(async () => {}),
    onExit: jest.fn(),
    isOneshot: () => false,
  })
  // Returns each result in turn, then repeats the last.
  const results = (...rs: HealthCheckResult['result'][]) => {
    let i = 0
    return jest.fn(
      (): HealthCheckResult => ({
        result: rs[Math.min(i++, rs.length - 1)],
        message: 'm',
      }),
    )
  }
  const launch = async (daemon: unknown, fn: jest.Mock) => {
    const hd = new HealthDaemon<Manifest>(
      daemon as any,
      [],
      'db',
      { display: 'Database', fn },
      fakeEffects(),
    )
    await hd.updateStatus()
    return hd
  }

  beforeEach(() => jest.useFakeTimers())
  afterEach(() => jest.useRealTimers())

  // Advances to each elapsed time and checks the cumulative poll count.
  const expectPolls = async (fn: jest.Mock, schedule: [number, number][]) => {
    let now = 0
    for (const [at, polls] of schedule) {
      await jest.advanceTimersByTimeAsync(at - now)
      now = at
      expect([at, fn.mock.calls.length]).toEqual([at, polls])
    }
  }

  it.each(['loading', 'waiting'] as const)(
    'catches a daemon that stops reporting %s within a second, then polls it every 30 seconds',
    async result => {
      const fn = results(result, 'success', result)
      const hd = await launch(fakeDaemon(), fn)

      await expectPolls(fn, [
        [1_000, 1],
        [2_000, 2],
        [31_000, 2],
        [32_000, 3],
        [61_000, 3],
        [62_000, 4],
      ])

      await hd.term()
    },
  )

  it.each(['loading', 'waiting'] as const)(
    'backs a daemon reporting %s off to 30 seconds before its first success',
    async result => {
      const fn = results(result)
      const hd = await launch(fakeDaemon(), fn)

      await expectPolls(fn, [
        [1_000, 1],
        [2_000, 2],
        [3_000, 2],
        [4_000, 3],
        [8_000, 4],
        [16_000, 5],
        [31_000, 5],
        [32_000, 6],
        [61_000, 6],
        [62_000, 7],
        [91_000, 7],
        [92_000, 8],
      ])

      await hd.term()
    },
  )

  it('runs a check with onceTrigger once and reports its failure', async () => {
    jest.advanceTimersByTime(1)
    const setHealth = jest.fn(async () => null)
    const fn = results('failure')
    const hd = new HealthDaemon<Manifest>(
      null,
      [],
      'signups',
      { display: 'Signups', gracePeriod: 0, trigger: onceTrigger, fn },
      { ...fakeEffects(), setHealth } as any,
    )
    await hd.updateStatus()

    await expectPolls(fn, [
      [0, 1],
      [60_000, 1],
    ])
    expect(setHealth.mock.calls.map(([h]: any) => h.result)).toEqual([
      'starting',
      'failure',
    ])

    await hd.term()
  })

  it.each(['loading', 'waiting'] as const)(
    'polls a standalone health check reporting %s every 30 seconds',
    async result => {
      const fn = results(result)
      const hd = await launch(null, fn)

      await expectPolls(fn, [
        [1_000, 1],
        [30_000, 1],
        [31_000, 2],
        [60_000, 2],
        [61_000, 3],
      ])

      await hd.term()
    },
  )
})

describe('HealthDaemon check loop', () => {
  const check = (trigger: Trigger) => {
    const fn = jest.fn(
      (): HealthCheckResult => ({ result: 'success', message: null }),
    )
    const hd = new HealthDaemon<Manifest>(
      null,
      [],
      'check',
      { display: 'Check', trigger, fn },
      fakeEffects(),
    )
    return { hd, fn }
  }
  const macrotask = () => new Promise(resolve => setImmediate(resolve))

  it('lets go of each trigger tick once its check has run', async () => {
    setFlagsFromString('--expose-gc')
    const gc: () => void = runInNewContext('gc')
    const ticks: WeakRef<object>[] = []
    let drained = () => {}
    const parked = new Promise<void>(resolve => (drained = resolve))
    const { hd, fn } = check(async function* () {
      for (let i = 0; i < 100; i++) {
        const tick = { i }
        ticks.push(new WeakRef(tick))
        yield tick
      }
      drained()
      await new Promise(() => {})
    })

    await hd.updateStatus()
    await parked
    await macrotask()
    gc()

    expect(fn).toHaveBeenCalledTimes(100)
    // V8's suspended async frame keeps the first tick and the loop the latest.
    expect(ticks.filter(tick => tick.deref()).length).toBeLessThanOrEqual(2)
    await hd.term()
  })

  it('ends a pending wait at once when stopped', async () => {
    jest.useFakeTimers()
    try {
      const { hd, fn } = check(cooldownTrigger(60_000))
      await hd.updateStatus()
      await jest.advanceTimersByTimeAsync(60_000)
      expect(fn).toHaveBeenCalledTimes(1)
      const signal: AbortSignal = (hd as any).session.abort.signal

      // Under fake timers the pending tick only ends through the abort.
      await hd.term()
      await jest.advanceTimersByTimeAsync(120_000)

      expect(fn).toHaveBeenCalledTimes(1)
      expect(getEventListeners(signal, 'abort')).toHaveLength(0)
    } finally {
      jest.useRealTimers()
    }
  })

  it('logs a trigger that throws and stops checking', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    let threw = () => {}
    const thrown = new Promise<void>(resolve => (threw = resolve))
    const { hd, fn } = check(async function* () {
      yield
      threw()
      throw new Error('broken trigger')
    })

    await hd.updateStatus()
    await thrown
    await macrotask()

    expect(fn).toHaveBeenCalledTimes(1)
    expect(logged).toHaveBeenCalledWith(
      'Daemon check health check failed: Error: broken trigger',
    )
    logged.mockRestore()
    await hd.term()
  })
})
