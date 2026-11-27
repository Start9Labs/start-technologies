import { getEventListeners } from 'node:events'
import { setFlagsFromString } from 'node:v8'
import { runInNewContext } from 'node:vm'
import { Daemon } from '../mainFn/Daemon'
import { Daemons } from '../mainFn/Daemons'
import { EXIT_SUCCESS, HealthDaemon } from '../mainFn/HealthDaemon'
import { HealthCheckResult } from '../health/checkFns'
import {
  cooldownTrigger,
  onceTrigger,
  statusTrigger,
  Trigger,
} from '../trigger'
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
    dependentsReady: true,
    ready: { display: 'Dep' },
    addWatcher: () => {},
    addDependentWatcher: () => {},
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
  const results = (
    ...rs: (HealthCheckResult | HealthCheckResult['result'])[]
  ) => {
    let i = 0
    return jest.fn((): HealthCheckResult => {
      const health = rs[Math.min(i++, rs.length - 1)]
      return typeof health === 'string'
        ? { result: health, message: 'm' }
        : health
    })
  }
  const pendingResults = [
    { result: 'loading', waiting: false, message: 'Syncing: 25%' },
    { result: 'loading', waiting: true, message: 'Peers unavailable' },
    { result: 'starting', waiting: true, message: 'Database unavailable' },
  ] as const
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

  const checks = [
    { kind: 'a daemon', daemon: fakeDaemon },
    { kind: 'a standalone check', daemon: () => null },
  ]
  const pendingChecks = checks.flatMap(c =>
    pendingResults.map(health => ({ ...c, health, ...health })),
  )

  it.each(pendingChecks)(
    'catches $kind leaving $result (waiting: $waiting) within a second, then polls it every 30 seconds',
    async ({ daemon, health }) => {
      const fn = results(health, 'success', health)
      const hd = await launch(daemon(), fn)

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

  it.each(pendingChecks)(
    'backs $kind reporting $result (waiting: $waiting) off to 30 seconds before its first success',
    async ({ daemon, health }) => {
      const fn = results(health)
      const hd = await launch(daemon(), fn)

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

  it.each(pendingResults)(
    'preserves $result messages and waiting flags',
    async health => {
      const setHealth = jest.fn(async () => null)
      const hd = new HealthDaemon<Manifest>(
        null,
        [],
        'sync',
        { display: 'Sync', trigger: onceTrigger, fn: () => health },
        { ...fakeEffects(), setHealth },
      )
      await hd.updateStatus()
      await jest.advanceTimersByTimeAsync(0)
      expect(setHealth).toHaveBeenLastCalledWith({
        ...health,
        id: 'sync',
        name: 'Sync',
      })
      await hd.term()
    },
  )

  it('marks dependency-blocked checks as starting and waiting', async () => {
    const dependency = new HealthDaemon<Manifest>(
      null,
      [],
      'db',
      { display: 'Database', fn: results('failure') },
      fakeEffects(),
    )
    const setHealth = jest.fn(async () => null)
    const daemon = fakeDaemon()
    const hd = new HealthDaemon<Manifest>(
      daemon as any,
      [dependency],
      'app',
      { display: 'Application', fn: results('success') },
      { ...fakeEffects(), setHealth },
    )
    await hd.updateStatus()
    expect(setHealth).toHaveBeenLastCalledWith({
      id: 'app',
      name: 'Application',
      result: 'starting',
      waiting: true,
      message: 'Database',
    })
    expect(daemon.start).not.toHaveBeenCalled()
    await hd.term()
    await dependency.term()
  })

  it('hides displayed probe errors during grace without masking readiness failures', async () => {
    const setHealth = jest.fn(async () => null)
    const hd = new HealthDaemon<Manifest>(
      fakeDaemon() as any,
      [],
      'db',
      { display: 'Database', fn: results('failure') },
      { ...fakeEffects(), setHealth },
    )
    await hd.updateStatus()
    await jest.advanceTimersByTimeAsync(1_000)
    expect(setHealth).toHaveBeenLastCalledWith({
      id: 'db',
      name: 'Database',
      result: 'starting',
      message: null,
    })
    expect(hd.health).toEqual({ result: 'failure', message: 'm' })
    expect(hd.isReady).toBe(false)
    await jest.advanceTimersByTimeAsync(9_000)
    expect(setHealth).toHaveBeenLastCalledWith({
      id: 'db',
      name: 'Database',
      result: 'failure',
      message: 'm',
    })
    await hd.term()
  })

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
      const signal: AbortSignal = (hd as any).session.signal

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

describe('HealthDaemon.term with a hung health check', () => {
  it('terminates the daemon without waiting, aborting the check', async () => {
    let signal: AbortSignal | undefined
    const daemon = {
      start: jest.fn(async () => {}),
      term: jest.fn(async () => {}),
      onExit: () => {},
      isOneshot: () => false,
    }
    const hd = new HealthDaemon<Manifest>(
      daemon as any,
      [],
      'svc',
      {
        display: 'Svc',
        trigger: cooldownTrigger(1),
        fn: s => {
          signal = s
          return new Promise<never>(() => {})
        },
      },
      fakeEffects(),
    )
    await hd.updateStatus()
    while (!signal) await tick()
    expect(signal.aborted).toBe(false)

    const outcome = await Promise.race([
      hd.term().then(() => 'stopped'),
      new Promise(resolve => setTimeout(() => resolve('hung'), 500)),
    ])

    expect(outcome).toBe('stopped')
    expect(daemon.term).toHaveBeenCalledTimes(1)
    expect(signal.aborted).toBe(true)
  })
})

describe('HealthDaemon ready-check hysteresis and display-null logging', () => {
  const controllableDaemon = () => {
    const exitFns: Array<(success: boolean) => void> = []
    return {
      start: jest.fn(async () => {}),
      stop: jest.fn(async () => {
        exitFns.length = 0
      }),
      term: jest.fn(async () => {
        exitFns.length = 0
      }),
      onExit: jest.fn((fn: (success: boolean) => void) => {
        exitFns.push(fn)
      }),
      isOneshot: () => false,
      lastExitError: null as Error | null,
      emitExit(success: boolean, error?: Error) {
        this.lastExitError = error ?? null
        exitFns.forEach(fn => fn(success))
      },
    }
  }

  beforeEach(() => jest.useFakeTimers())
  afterEach(() => {
    jest.restoreAllMocks()
    jest.useRealTimers()
  })

  it('tolerates transient failures on defaultTrigger, retries at 1s, resets on recovery, and logs with display: null', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const setHealth = jest.fn(async () => null)
    const depDaemon = controllableDaemon()
    const appDaemon = controllableDaemon()
    const sequence: HealthCheckResult[] = [
      { result: 'success', message: null },
      { result: 'failure', message: 'pg_isready timeout 1' },
      { result: 'failure', message: 'pg_isready timeout 2' },
      { result: 'success', message: null },
      { result: 'failure', message: 'pg_isready timeout 3' },
      { result: 'success', message: null },
    ]
    let i = 0
    const depFn = jest.fn(() => sequence[Math.min(i++, sequence.length - 1)])

    const dep = new HealthDaemon<Manifest>(
      depDaemon as unknown as Daemon<Manifest>,
      [],
      'postgres',
      { display: null, fn: depFn },
      { ...fakeEffects(), setHealth },
    )
    const app = new HealthDaemon<Manifest>(
      appDaemon as unknown as Daemon<Manifest>,
      [dep],
      'immich-server',
      {
        display: 'Immich Server',
        fn: () => ({ result: 'success', message: null }),
      },
      fakeEffects(),
    )

    await dep.updateStatus()
    await app.updateStatus()
    expect(appDaemon.start).not.toHaveBeenCalled()

    await jest.advanceTimersByTimeAsync(1_000)
    expect(depFn).toHaveBeenCalledTimes(1)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)
    expect(appDaemon.stop).not.toHaveBeenCalled()

    await jest.advanceTimersByTimeAsync(30_000)
    expect(depFn).toHaveBeenCalledTimes(2)
    expect(appDaemon.stop).not.toHaveBeenCalled()
    expect(setHealth).not.toHaveBeenCalled()
    expect(logged).toHaveBeenCalledWith(
      'Health Check postgres failed:',
      'pg_isready timeout 1',
    )

    await jest.advanceTimersByTimeAsync(1_000)
    expect(depFn).toHaveBeenCalledTimes(3)
    expect(appDaemon.stop).not.toHaveBeenCalled()
    expect(logged).toHaveBeenCalledWith(
      'Health Check postgres failed:',
      'pg_isready timeout 2',
    )

    await jest.advanceTimersByTimeAsync(1_000)
    expect(depFn).toHaveBeenCalledTimes(4)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)
    expect(appDaemon.stop).not.toHaveBeenCalled()

    await jest.advanceTimersByTimeAsync(29_000)
    expect(depFn).toHaveBeenCalledTimes(4)
    await jest.advanceTimersByTimeAsync(1_000)
    expect(depFn).toHaveBeenCalledTimes(5)
    expect(appDaemon.stop).not.toHaveBeenCalled()

    await jest.advanceTimersByTimeAsync(1_000)
    expect(depFn).toHaveBeenCalledTimes(6)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)
    expect(appDaemon.stop).not.toHaveBeenCalled()

    await app.term()
    await dep.term()
  })

  it('pauses dependents on the third consecutive failure (including async and sync throws) and resumes on recovery', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const depDaemon = controllableDaemon()
    const appDaemon = controllableDaemon()
    const steps: Array<() => Promise<HealthCheckResult> | HealthCheckResult> = [
      () => ({ result: 'success', message: null }),
      () => Promise.reject(new Error('exec ETIMEDOUT')),
      () => {
        throw 'sync check boom'
      },
      () => Promise.reject({ result: 'failure', message: 'script failed' }),
      () => ({ result: 'success', message: null }),
    ]
    let i = 0
    const depFn = jest.fn(() => steps[Math.min(i++, steps.length - 1)]())

    const dep = new HealthDaemon<Manifest>(
      depDaemon as unknown as Daemon<Manifest>,
      [],
      'postgres',
      { display: null, fn: depFn },
      fakeEffects(),
    )
    const app = new HealthDaemon<Manifest>(
      appDaemon as unknown as Daemon<Manifest>,
      [dep],
      'immich-server',
      EXIT_SUCCESS,
      fakeEffects(),
    )

    await dep.updateStatus()
    await app.updateStatus()

    await jest.advanceTimersByTimeAsync(1_000)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)

    await jest.advanceTimersByTimeAsync(30_000)
    expect(appDaemon.stop).not.toHaveBeenCalled()
    expect(logged).toHaveBeenCalledWith(
      'Health Check postgres failed:',
      'exec ETIMEDOUT',
    )

    await jest.advanceTimersByTimeAsync(1_000)
    expect(appDaemon.stop).not.toHaveBeenCalled()
    expect(logged).toHaveBeenCalledWith(
      'Health Check postgres failed:',
      'sync check boom',
    )

    await jest.advanceTimersByTimeAsync(1_000)
    expect(appDaemon.stop).toHaveBeenCalledTimes(1)
    expect(appDaemon.term).not.toHaveBeenCalled()
    expect(logged).toHaveBeenCalledWith(
      'Health Check postgres failed:',
      'script failed',
    )

    await jest.advanceTimersByTimeAsync(1_000)
    expect(appDaemon.start).toHaveBeenCalledTimes(2)

    await app.term()
    await dep.term()
  })

  it.each([
    { label: 'crash', exitOk: false, error: new Error('SIGSEGV') },
    { label: 'unexpected clean exit', exitOk: true, error: undefined },
  ])(
    'pauses dependents immediately on daemon $label even while a transient failure is active',
    async ({ exitOk, error }) => {
      const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
      const depDaemon = controllableDaemon()
      const appDaemon = controllableDaemon()
      const resultsSeq: HealthCheckResult[] = [
        { result: 'success', message: null },
        { result: 'failure', message: 'transient blip' },
      ]
      let i = 0
      const dep = new HealthDaemon<Manifest>(
        depDaemon as unknown as Daemon<Manifest>,
        [],
        'postgres',
        {
          display: null,
          fn: () => resultsSeq[Math.min(i++, resultsSeq.length - 1)],
        },
        fakeEffects(),
      )
      const app = new HealthDaemon<Manifest>(
        appDaemon as unknown as Daemon<Manifest>,
        [dep],
        'immich-server',
        EXIT_SUCCESS,
        fakeEffects(),
      )

      await dep.updateStatus()
      await app.updateStatus()

      await jest.advanceTimersByTimeAsync(1_000)
      expect(appDaemon.start).toHaveBeenCalledTimes(1)

      await jest.advanceTimersByTimeAsync(30_000)
      expect(dep.health.result).toBe('failure')
      expect(appDaemon.stop).not.toHaveBeenCalled()

      depDaemon.emitExit(exitOk, error)
      await jest.advanceTimersByTimeAsync(0)
      expect(appDaemon.stop).toHaveBeenCalledTimes(1)
      expect(logged).toHaveBeenCalledWith(
        'Health Check postgres failed:',
        expect.stringContaining('postgres daemon'),
      )

      await app.term()
      await dep.term()
    },
  )

  it('respects gracePeriod before logging failures for display: null', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const setHealth = jest.fn(async () => null)
    const dep = new HealthDaemon<Manifest>(
      controllableDaemon() as unknown as Daemon<Manifest>,
      [],
      'postgres',
      {
        display: null,
        fn: () => ({ result: 'failure', message: 'still booting' }),
      },
      { ...fakeEffects(), setHealth },
    )

    await dep.updateStatus()
    await jest.advanceTimersByTimeAsync(1_000)
    expect(logged).not.toHaveBeenCalled()
    expect(setHealth).not.toHaveBeenCalled()

    await jest.advanceTimersByTimeAsync(9_000)
    expect(logged).toHaveBeenCalledWith(
      'Health Check postgres failed:',
      'still booting',
    )
    expect(setHealth).not.toHaveBeenCalled()

    await dep.term()
  })

  it('does not deadlock a multi-dependency child when one ready dependency has a transient failure as another becomes ready', async () => {
    jest.spyOn(console, 'error').mockImplementation(() => {})
    let aResult: HealthCheckResult = { result: 'success', message: null }
    let bResult: HealthCheckResult = { result: 'starting', message: null }
    const cDaemon = controllableDaemon()

    const depA = new HealthDaemon<Manifest>(
      controllableDaemon() as unknown as Daemon<Manifest>,
      [],
      'dep-a',
      { display: null, fn: () => aResult },
      fakeEffects(),
    )
    const depB = new HealthDaemon<Manifest>(
      controllableDaemon() as unknown as Daemon<Manifest>,
      [],
      'dep-b',
      { display: null, trigger: cooldownTrigger(500), fn: () => bResult },
      fakeEffects(),
    )
    const child = new HealthDaemon<Manifest>(
      cDaemon as unknown as Daemon<Manifest>,
      [depA, depB],
      'child',
      EXIT_SUCCESS,
      fakeEffects(),
    )

    await depA.updateStatus()
    await depB.updateStatus()
    await child.updateStatus()

    await jest.advanceTimersByTimeAsync(1_000)
    expect(cDaemon.start).not.toHaveBeenCalled()

    aResult = { result: 'failure', message: 'blip' }
    await jest.advanceTimersByTimeAsync(30_000)
    expect(depA.health.result).toBe('failure')
    expect(cDaemon.start).not.toHaveBeenCalled()

    bResult = { result: 'success', message: null }
    await jest.advanceTimersByTimeAsync(500)
    expect(depA.health.result).toBe('failure')
    expect(cDaemon.start).toHaveBeenCalledTimes(1)

    aResult = { result: 'success', message: null }
    await jest.advanceTimersByTimeAsync(500)
    expect(depA.health.result).toBe('success')
    expect(cDaemon.start).toHaveBeenCalledTimes(1)
    expect(cDaemon.stop).not.toHaveBeenCalled()

    await child.term()
    await depB.term()
    await depA.term()
  })

  it('pauses dependents if a finite trigger ends while a transient failure is pending', async () => {
    jest.spyOn(console, 'error').mockImplementation(() => {})
    const depDaemon = controllableDaemon()
    const appDaemon = controllableDaemon()
    let call = 0
    const dep = new HealthDaemon<Manifest>(
      depDaemon as unknown as Daemon<Manifest>,
      [],
      'finite',
      {
        display: null,
        gracePeriod: 0,
        trigger: async function* () {
          yield
          await new Promise(resolve => setTimeout(resolve, 1_000))
          yield
        },
        fn: () =>
          call++ === 0
            ? { result: 'success', message: null }
            : { result: 'failure', message: 'last poll failed' },
      },
      fakeEffects(),
    )
    const app = new HealthDaemon<Manifest>(
      appDaemon as unknown as Daemon<Manifest>,
      [dep],
      'app',
      EXIT_SUCCESS,
      fakeEffects(),
    )

    await dep.updateStatus()
    await app.updateStatus()
    await jest.advanceTimersByTimeAsync(0)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)

    await jest.advanceTimersByTimeAsync(1_000)
    expect(appDaemon.stop).toHaveBeenCalledTimes(1)

    await app.term()
    await dep.term()
  })

  it('propagates upstream dependency loss through a middle daemon immediately before middle stop finishes', async () => {
    jest.spyOn(console, 'error').mockImplementation(() => {})
    let releaseMidStop: (() => void) | null = null
    const rootDaemon = controllableDaemon()
    const midDaemon = {
      ...controllableDaemon(),
      stop: jest.fn(async () => {
        await new Promise<void>(resolve => (releaseMidStop = resolve))
      }),
    }
    const leafDaemon = controllableDaemon()

    const root = new HealthDaemon<Manifest>(
      rootDaemon as unknown as Daemon<Manifest>,
      [],
      'root',
      { display: null, fn: () => ({ result: 'success', message: null }) },
      fakeEffects(),
    )
    const mid = new HealthDaemon<Manifest>(
      midDaemon as unknown as Daemon<Manifest>,
      [root],
      'mid',
      { display: null, fn: () => ({ result: 'success', message: null }) },
      fakeEffects(),
    )
    const leaf = new HealthDaemon<Manifest>(
      leafDaemon as unknown as Daemon<Manifest>,
      [mid],
      'leaf',
      EXIT_SUCCESS,
      fakeEffects(),
    )

    await root.updateStatus()
    await mid.updateStatus()
    await leaf.updateStatus()

    await jest.advanceTimersByTimeAsync(2_000)
    expect(leafDaemon.start).toHaveBeenCalledTimes(1)

    rootDaemon.emitExit(false, new Error('killed'))
    await jest.advanceTimersByTimeAsync(0)
    expect(midDaemon.stop).toHaveBeenCalledTimes(1)
    expect(releaseMidStop).not.toBeNull()
    expect(leafDaemon.stop).toHaveBeenCalledTimes(1)

    releaseMidStop!()
    await jest.advanceTimersByTimeAsync(0)
    expect(leafDaemon.start).toHaveBeenCalledTimes(1)
    expect(leafDaemon.stop).toHaveBeenCalledTimes(1)

    await leaf.term()
    await mid.term()
    await root.term()
  })

  it('pauses dependents if a throwing trigger stops while a transient failure is pending', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const depDaemon = controllableDaemon()
    const appDaemon = controllableDaemon()
    let call = 0
    const dep = new HealthDaemon<Manifest>(
      depDaemon as unknown as Daemon<Manifest>,
      [],
      'broken',
      {
        display: null,
        gracePeriod: 0,
        trigger: async function* () {
          yield
          await new Promise(resolve => setTimeout(resolve, 1_000))
          yield
          throw new Error('trigger exploded')
        },
        fn: () =>
          call++ === 0
            ? { result: 'success', message: null }
            : { result: 'failure', message: 'blip before crash' },
      },
      fakeEffects(),
    )
    const app = new HealthDaemon<Manifest>(
      appDaemon as unknown as Daemon<Manifest>,
      [dep],
      'app',
      EXIT_SUCCESS,
      fakeEffects(),
    )

    await dep.updateStatus()
    await app.updateStatus()
    await jest.advanceTimersByTimeAsync(0)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)

    await jest.advanceTimersByTimeAsync(1_000)
    expect(appDaemon.stop).toHaveBeenCalledTimes(1)
    expect(logged).toHaveBeenCalledWith(
      'Daemon broken health check failed: Error: trigger exploded',
    )

    await app.term()
    await dep.term()
  })

  it('discards an in-flight middle poll that resolves after upstream loss without relaunching the leaf', async () => {
    jest.spyOn(console, 'error').mockImplementation(() => {})
    const rootDaemon = controllableDaemon()
    const midDaemon = controllableDaemon()
    const leafDaemon = controllableDaemon()
    let releaseMidSetHealth: (() => void) | null = null
    let holdNextMidSetHealth = false
    let releaseMidPoll: (() => void) | null = null
    let holdNextMidPoll = false

    const midEffects = {
      ...fakeEffects(),
      setHealth: jest.fn(async () => {
        if (!holdNextMidSetHealth) return null
        holdNextMidSetHealth = false
        await new Promise<void>(resolve => (releaseMidSetHealth = resolve))
        return null
      }),
    }

    const root = new HealthDaemon<Manifest>(
      rootDaemon as unknown as Daemon<Manifest>,
      [],
      'root',
      { display: null, fn: () => ({ result: 'success', message: null }) },
      fakeEffects(),
    )
    const mid = new HealthDaemon<Manifest>(
      midDaemon as unknown as Daemon<Manifest>,
      [root],
      'mid',
      {
        display: 'Mid',
        fn: async () => {
          if (holdNextMidPoll) {
            holdNextMidPoll = false
            await new Promise<void>(resolve => (releaseMidPoll = resolve))
          }
          return { result: 'success', message: null }
        },
      },
      midEffects,
    )
    const leaf = new HealthDaemon<Manifest>(
      leafDaemon as unknown as Daemon<Manifest>,
      [mid],
      'leaf',
      EXIT_SUCCESS,
      fakeEffects(),
    )

    await root.updateStatus()
    await mid.updateStatus()
    await leaf.updateStatus()
    await jest.advanceTimersByTimeAsync(2_000)
    expect(leafDaemon.start).toHaveBeenCalledTimes(1)

    holdNextMidPoll = true
    await jest.advanceTimersByTimeAsync(30_000)
    expect(releaseMidPoll).not.toBeNull()

    holdNextMidSetHealth = true
    rootDaemon.emitExit(false, new Error('killed'))
    await jest.advanceTimersByTimeAsync(0)
    expect(releaseMidSetHealth).not.toBeNull()
    expect(leafDaemon.stop).toHaveBeenCalledTimes(1)

    releaseMidPoll!()
    await jest.advanceTimersByTimeAsync(0)
    expect(leafDaemon.start).toHaveBeenCalledTimes(1)
    expect(leafDaemon.stop).toHaveBeenCalledTimes(1)

    releaseMidSetHealth!()
    await jest.advanceTimersByTimeAsync(0)
    expect(midDaemon.stop).toHaveBeenCalledTimes(1)
    expect(leafDaemon.start).toHaveBeenCalledTimes(1)
    expect(leafDaemon.stop).toHaveBeenCalledTimes(1)

    await leaf.term()
    await mid.term()
    await root.term()
  })

  it('discards a stale pre-crash probe so dependents stay paused until a fresh post-crash success', async () => {
    jest.spyOn(console, 'error').mockImplementation(() => {})
    const depDaemon = controllableDaemon()
    const appDaemon = controllableDaemon()
    let releaseStaleProbe: (() => void) | null = null
    let poll = 0
    const dep = new HealthDaemon<Manifest>(
      depDaemon as unknown as Daemon<Manifest>,
      [],
      'postgres',
      {
        display: null,
        fn: async () => {
          poll++
          if (poll === 1) return { result: 'success', message: null }
          if (poll === 2) {
            await new Promise<void>(resolve => (releaseStaleProbe = resolve))
            return { result: 'success', message: null }
          }
          if (poll === 3) {
            return { result: 'failure', message: 'still restarting' }
          }
          return { result: 'success', message: null }
        },
      },
      fakeEffects(),
    )
    const app = new HealthDaemon<Manifest>(
      appDaemon as unknown as Daemon<Manifest>,
      [dep],
      'app',
      EXIT_SUCCESS,
      fakeEffects(),
    )

    await dep.updateStatus()
    await app.updateStatus()
    await jest.advanceTimersByTimeAsync(1_000)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)

    await jest.advanceTimersByTimeAsync(30_000)
    expect(releaseStaleProbe).not.toBeNull()

    depDaemon.emitExit(false, new Error('SIGSEGV'))
    await jest.advanceTimersByTimeAsync(0)
    expect(appDaemon.stop).toHaveBeenCalledTimes(1)

    releaseStaleProbe!()
    await jest.advanceTimersByTimeAsync(0)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)

    await jest.advanceTimersByTimeAsync(1_000)
    expect(poll).toBe(3)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)

    await jest.advanceTimersByTimeAsync(1_000)
    expect(poll).toBe(4)
    expect(appDaemon.start).toHaveBeenCalledTimes(2)

    await app.term()
    await dep.term()
  })

  it('keeps lifecycle order when waiting health publication finishes late or rejects', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const depDaemon = controllableDaemon()
    const appDaemon = controllableDaemon()
    let depResult: HealthCheckResult = { result: 'success', message: null }
    let releaseWaitingWrite: (() => void) | null = null
    let mode: 'normal' | 'delay-waiting' | 'reject-waiting' = 'normal'

    const appEffects = {
      ...fakeEffects(),
      setHealth: jest.fn(async (health: { waiting?: boolean }) => {
        if (health.waiting && mode === 'delay-waiting') {
          mode = 'normal'
          await new Promise<void>(resolve => (releaseWaitingWrite = resolve))
          return null
        }
        if (health.waiting && mode === 'reject-waiting') {
          mode = 'normal'
          throw new Error('rpc disconnected')
        }
        return null
      }),
    }

    const dep = new HealthDaemon<Manifest>(
      depDaemon as unknown as Daemon<Manifest>,
      [],
      'postgres',
      { display: 'Postgres', fn: () => depResult },
      fakeEffects(),
    )
    const app = new HealthDaemon<Manifest>(
      appDaemon as unknown as Daemon<Manifest>,
      [dep],
      'app',
      { display: 'App', fn: () => ({ result: 'success', message: null }) },
      appEffects as unknown as T.Effects,
    )

    await dep.updateStatus()
    await app.updateStatus()
    await jest.advanceTimersByTimeAsync(1_000)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)

    mode = 'delay-waiting'
    depDaemon.emitExit(false, new Error('crash 1'))
    await jest.advanceTimersByTimeAsync(0)
    expect(releaseWaitingWrite).not.toBeNull()
    expect(appDaemon.stop).toHaveBeenCalledTimes(1)

    depResult = { result: 'success', message: null }
    await jest.advanceTimersByTimeAsync(30_000)
    expect(appDaemon.start).toHaveBeenCalledTimes(2)

    releaseWaitingWrite!()
    await jest.advanceTimersByTimeAsync(0)
    expect(appDaemon.stop).toHaveBeenCalledTimes(1)
    expect(appDaemon.start).toHaveBeenCalledTimes(2)

    mode = 'reject-waiting'
    depDaemon.emitExit(false, new Error('crash 2'))
    await jest.advanceTimersByTimeAsync(0)
    expect(appDaemon.stop).toHaveBeenCalledTimes(2)
    expect(logged).toHaveBeenCalledWith(
      'Failed to report health for app:',
      expect.any(Error),
    )

    await app.term()
    await dep.term()
  })

  it.each([
    {
      label: 'failure: Infinity',
      trigger: () => statusTrigger(1_000, { failure: Infinity }),
      second: { result: 'failure', message: 'fatal' } as HealthCheckResult,
    },
    {
      label: 'disabled: Infinity',
      trigger: () => statusTrigger(1_000, { disabled: Infinity }),
      second: {
        result: 'disabled',
        message: 'turned off',
      } as HealthCheckResult,
    },
  ])(
    'pauses dependents immediately when statusTrigger parks on $label',
    async ({ trigger, second }) => {
      jest.spyOn(console, 'error').mockImplementation(() => {})
      const depDaemon = controllableDaemon()
      const appDaemon = controllableDaemon()
      let poll = 0
      const dep = new HealthDaemon<Manifest>(
        depDaemon as unknown as Daemon<Manifest>,
        [],
        'dep',
        {
          display: null,
          gracePeriod: 0,
          trigger: trigger(),
          fn: () =>
            poll++ === 0 ? { result: 'success', message: null } : second,
        },
        fakeEffects(),
      )
      const app = new HealthDaemon<Manifest>(
        appDaemon as unknown as Daemon<Manifest>,
        [dep],
        'app',
        EXIT_SUCCESS,
        fakeEffects(),
      )

      await dep.updateStatus()
      await app.updateStatus()
      await jest.advanceTimersByTimeAsync(1_000)
      expect(appDaemon.start).toHaveBeenCalledTimes(1)

      await jest.advanceTimersByTimeAsync(1_000)
      expect(poll).toBe(2)
      expect(appDaemon.stop).toHaveBeenCalledTimes(1)

      await app.term()
      await dep.term()
    },
  )

  it('keeps runUntilSuccess pending while a previously ready check is reporting failure', async () => {
    jest.spyOn(console, 'error').mockImplementation(() => {})
    let aResult: HealthCheckResult = { result: 'success', message: null }
    let bResult: HealthCheckResult = { result: 'starting', message: null }
    const effects = fakeEffects()
    const chain = Daemons.of<Manifest>({ effects })
      .addHealthCheck('check-a', {
        ready: {
          display: null,
          gracePeriod: 0,
          trigger: cooldownTrigger(1_000),
          fn: () => aResult,
        },
        requires: [],
      })
      .addHealthCheck('check-b', {
        ready: {
          display: null,
          gracePeriod: 0,
          trigger: cooldownTrigger(500),
          fn: () => bResult,
        },
        requires: [],
      })

    let done = false
    const running = chain.runUntilSuccess(null).then(() => {
      done = true
    })

    await jest.advanceTimersByTimeAsync(1_000)
    expect(done).toBe(false)

    aResult = { result: 'failure', message: 'transient' }
    await jest.advanceTimersByTimeAsync(1_000)
    expect(done).toBe(false)

    bResult = { result: 'success', message: null }
    await jest.advanceTimersByTimeAsync(500)
    expect(done).toBe(false)

    aResult = { result: 'success', message: null }
    await jest.advanceTimersByTimeAsync(500)
    await running
    expect(done).toBe(true)
  })

  it.each<HealthCheckResult>([
    { result: 'loading', message: 'catching up' },
    { result: 'disabled', message: 'turned off' },
    { result: 'starting', message: 'initializing' },
    { result: 'starting', waiting: true, message: 'waiting' },
  ])(
    'delivers raw transitions and pauses on $result after a tolerated failure',
    async nonFailure => {
      jest.spyOn(console, 'error').mockImplementation(() => {})
      const depDaemon = controllableDaemon()
      const appDaemon = controllableDaemon()
      const seq: HealthCheckResult[] = [
        { result: 'success', message: null },
        { result: 'failure', message: 'blip 1' },
        nonFailure,
        { result: 'success', message: null },
      ]
      let i = 0
      const dep = new HealthDaemon<Manifest>(
        depDaemon as unknown as Daemon<Manifest>,
        [],
        'dep',
        {
          display: null,
          trigger: cooldownTrigger(1_000),
          fn: () => seq[Math.min(i++, seq.length - 1)],
        },
        fakeEffects(),
      )
      const app = new HealthDaemon<Manifest>(
        appDaemon as unknown as Daemon<Manifest>,
        [dep],
        'app',
        EXIT_SUCCESS,
        fakeEffects(),
      )
      const observed: string[] = []
      dep.addWatcher(() => observed.push(dep.health.result))

      await dep.updateStatus()
      await app.updateStatus()

      await jest.advanceTimersByTimeAsync(2_000)
      expect(observed).toEqual(['success', 'failure'])
      expect(appDaemon.start).toHaveBeenCalledTimes(1)
      expect(appDaemon.stop).not.toHaveBeenCalled()

      await jest.advanceTimersByTimeAsync(1_000)
      expect(dep.health.result).toBe(nonFailure.result)
      expect(appDaemon.stop).toHaveBeenCalledTimes(1)

      await jest.advanceTimersByTimeAsync(1_000)
      expect(observed).toEqual([
        'success',
        'failure',
        nonFailure.result,
        'success',
      ])
      expect(appDaemon.start).toHaveBeenCalledTimes(2)
      expect(appDaemon.stop).toHaveBeenCalledTimes(1)

      await app.term()
      await dep.term()
    },
  )

  it('handles unstringifiable thrown values without terminating the health check loop', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const depDaemon = controllableDaemon()
    let poll = 0
    const dep = new HealthDaemon<Manifest>(
      depDaemon as unknown as Daemon<Manifest>,
      [],
      'dep',
      {
        display: null,
        gracePeriod: 0,
        trigger: cooldownTrigger(1_000),
        fn: () => {
          poll++
          if (poll === 1) return { result: 'success', message: null }
          if (poll === 2) throw Object.create(null)
          if (poll === 3) throw { message: Object.create(null) }
          return { result: 'success', message: null }
        },
      },
      fakeEffects(),
    )

    await dep.updateStatus()
    await jest.advanceTimersByTimeAsync(4_000)
    expect(poll).toBe(4)
    expect(dep.health.result).toBe('success')
    expect(logged).toHaveBeenCalledWith(
      'Health Check dep failed:',
      'Unknown health check error',
    )

    await dep.term()
  })

  it('handles unstringifiable trigger errors cleanly without rejecting term()', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const depDaemon = controllableDaemon()
    const appDaemon = controllableDaemon()
    let call = 0
    const dep = new HealthDaemon<Manifest>(
      depDaemon as unknown as Daemon<Manifest>,
      [],
      'dep',
      {
        display: null,
        gracePeriod: 0,
        trigger: async function* () {
          yield
          await new Promise(resolve => setTimeout(resolve, 1_000))
          yield
          throw Object.create(null)
        },
        fn: () =>
          call++ === 0
            ? { result: 'success', message: null }
            : {
                result: 'failure',
                message: 'blip before unstringifiable throw',
              },
      },
      fakeEffects(),
    )
    const app = new HealthDaemon<Manifest>(
      appDaemon as unknown as Daemon<Manifest>,
      [dep],
      'app',
      EXIT_SUCCESS,
      fakeEffects(),
    )

    await dep.updateStatus()
    await app.updateStatus()
    await jest.advanceTimersByTimeAsync(0)
    expect(appDaemon.start).toHaveBeenCalledTimes(1)

    await jest.advanceTimersByTimeAsync(1_000)
    expect(appDaemon.stop).toHaveBeenCalledTimes(1)
    expect(logged).toHaveBeenCalledWith(
      'Daemon dep health check failed: Unknown health check error',
    )

    await expect(app.term()).resolves.toBeUndefined()
    await expect(dep.term()).resolves.toBeUndefined()
    expect(depDaemon.term).toHaveBeenCalledTimes(1)
  })

  it('pauses without awaiting a hung ready.fn and ignores its late rejection across resume and active hysteresis', async () => {
    const logged = jest.spyOn(console, 'error').mockImplementation(() => {})
    const rootDaemon = controllableDaemon()
    const midDaemon = controllableDaemon()
    const leafDaemon = controllableDaemon()
    let rootResult: HealthCheckResult = { result: 'success', message: null }
    let rejectOldProbe: ((err: Error) => void) | null = null
    let firstSignal: AbortSignal | undefined
    let secondSignal: AbortSignal | undefined
    let midPoll = 0

    const root = new HealthDaemon<Manifest>(
      rootDaemon as unknown as Daemon<Manifest>,
      [],
      'root',
      { display: null, fn: () => rootResult },
      fakeEffects(),
    )
    const mid = new HealthDaemon<Manifest>(
      midDaemon as unknown as Daemon<Manifest>,
      [root],
      'mid',
      {
        display: null,
        gracePeriod: 0,
        fn: signal => {
          midPoll++
          if (midPoll === 1) {
            firstSignal = signal
            return { result: 'success', message: null }
          }
          if (midPoll === 2) {
            return new Promise<HealthCheckResult>((_resolve, reject) => {
              rejectOldProbe = reject
            })
          }
          if (midPoll === 3) {
            secondSignal = signal
            return { result: 'success', message: null }
          }
          if (midPoll === 4 || midPoll === 5) {
            return { result: 'failure', message: `session2 blip ${midPoll}` }
          }
          return { result: 'success', message: null }
        },
      },
      fakeEffects(),
    )
    const leaf = new HealthDaemon<Manifest>(
      leafDaemon as unknown as Daemon<Manifest>,
      [mid],
      'leaf',
      EXIT_SUCCESS,
      fakeEffects(),
    )

    await root.updateStatus()
    await mid.updateStatus()
    await leaf.updateStatus()
    await jest.advanceTimersByTimeAsync(2_000)
    expect(leafDaemon.start).toHaveBeenCalledTimes(1)
    expect(firstSignal?.aborted).toBe(false)

    // Start second poll on mid that stays hung in-flight
    await jest.advanceTimersByTimeAsync(30_000)
    expect(midPoll).toBe(2)
    expect(rejectOldProbe).not.toBeNull()

    // Crash root: mid must stop immediately without waiting for the hung probe
    rootDaemon.emitExit(false, new Error('killed'))
    await jest.advanceTimersByTimeAsync(0)
    expect(firstSignal?.aborted).toBe(true)
    expect(midDaemon.stop).toHaveBeenCalledTimes(1)
    expect(leafDaemon.stop).toHaveBeenCalledTimes(1)

    // Root recovers on its next 30s tick -> mid resumes with a fresh signal and leaf relaunches
    rootResult = { result: 'success', message: null }
    await jest.advanceTimersByTimeAsync(29_000)
    await jest.advanceTimersByTimeAsync(1_000)
    expect(midPoll).toBe(3)
    expect(secondSignal).toBeDefined()
    expect(secondSignal).not.toBe(firstSignal)
    expect(secondSignal?.aborted).toBe(false)
    expect(leafDaemon.start).toHaveBeenCalledTimes(2)

    // Two transient failures in session 2 (hysteresis counter = 2, below threshold 3)
    await jest.advanceTimersByTimeAsync(30_000)
    expect(midPoll).toBe(4)
    await jest.advanceTimersByTimeAsync(1_000)
    expect(midPoll).toBe(5)
    expect(leafDaemon.stop).toHaveBeenCalledTimes(1)

    // Now the aborted session-1 probe finally rejects: must not log, increment
    // session 2's failure count, or trigger post-loop dependentsReady reset.
    rejectOldProbe!(new Error('aborted old exec'))
    await jest.advanceTimersByTimeAsync(0)
    expect(leafDaemon.stop).toHaveBeenCalledTimes(1)
    expect(logged).not.toHaveBeenCalledWith(
      'Health Check mid failed:',
      'aborted old exec',
    )

    // Session 2 recovers on next poll; leaf stayed running throughout
    await jest.advanceTimersByTimeAsync(1_000)
    expect(midPoll).toBe(6)
    expect(leafDaemon.stop).toHaveBeenCalledTimes(1)
    expect(leafDaemon.start).toHaveBeenCalledTimes(2)

    await leaf.term()
    await mid.term()
    await root.term()
  })
})
