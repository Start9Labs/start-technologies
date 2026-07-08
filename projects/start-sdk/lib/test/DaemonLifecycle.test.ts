import { Daemon } from '../mainFn/Daemon'
import { EXIT_SUCCESS, HealthDaemon } from '../mainFn/HealthDaemon'
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
  destroyPending = false
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
      if (this.holdCount === 0 && this.destroyPending) await this._destroy()
    }
  }
  async destroy(): Promise<void> {
    this.destroyPending = true
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
