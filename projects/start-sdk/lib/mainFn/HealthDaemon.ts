import { HealthCheckResult } from '../health/checkFns'
import { withGracePeriod } from '../health/isInGracePeriod'
import { defaultTrigger } from '../trigger/defaultTrigger'
import {
  isTerminalResult,
  TerminalAwareTrigger,
} from '../trigger/statusTrigger'
import { Ready } from './Daemons'
import { Daemon } from './Daemon'
import { Effects, SDKManifest } from '@start9labs/start-core/types'
import { asError } from '@start9labs/start-core/util/asError'

export const EXIT_SUCCESS = 'EXIT_SUCCESS' as const
const READY_FAILURE_THRESHOLD = 3

function safeString(value: unknown): string {
  try {
    return String(value)
  } catch {
    return 'Unknown health check error'
  }
}

function formatError(err: unknown): string {
  try {
    const raw =
      typeof err === 'object' && err !== null && 'message' in err
        ? (err as { message: unknown }).message
        : err
    return safeString(raw)
  } catch {
    return 'Unknown health check error'
  }
}

export class HealthDaemon<Manifest extends SDKManifest> {
  private _health: HealthCheckResult = {
    result: 'starting',
    waiting: true,
    message: null,
  }
  private healthWatchers: Array<{
    fn: () => unknown
    kind: 'health' | 'dependents'
  }> = []
  private running = false
  private terminated = false
  private transition: Promise<void> = Promise.resolve()
  private started?: number
  private resolveReady: (() => void) | undefined
  private resolvedReady: boolean = false
  private readyPromise: Promise<void>
  private session: { abort: AbortController; done: Promise<void> } | null = null
  private dependentsReady = false
  private consecutiveUnhealthy = 0
  private probeEpoch = 0
  constructor(
    readonly daemon: Daemon<Manifest> | null,
    readonly dependencies: HealthDaemon<Manifest>[],
    readonly id: string,
    readonly ready: Ready | typeof EXIT_SUCCESS,
    readonly effects: Effects,
    private readonly strictDependencies = false,
  ) {
    this.readyPromise = new Promise(
      resolve =>
        (this.resolveReady = () => {
          resolve()
          this.resolvedReady = true
        }),
    )
    this.dependencies.forEach(d => {
      if (this.strictDependencies) {
        d.addWatcher(() => void this.updateStatus())
      } else {
        d.addDependentWatcher(() => void this.updateStatus())
      }
    })
  }

  async term(termOptions?: {
    signal?: NodeJS.Signals | undefined
    timeout?: number | undefined
  }) {
    this.terminated = true
    this.healthWatchers = []
    await this.transition.catch(() => {})
    this.running = false
    await this.stopSession()

    await this.daemon?.term({
      ...termOptions,
    })
  }

  addWatcher(watcher: () => unknown) {
    this.healthWatchers.push({ fn: watcher, kind: 'health' })
  }

  private addDependentWatcher(watcher: () => unknown) {
    this.healthWatchers.push({ fn: watcher, kind: 'dependents' })
  }

  get health() {
    return Object.freeze(this._health)
  }

  // A draining daemon ignores start(); pauses must finish before resumes.
  private changeRunning(newStatus: boolean): Promise<void> {
    const next = this.transition
      .catch(() => {})
      .then(() => this.applyRunning(newStatus))
    this.transition = next
    return next
  }

  private async applyRunning(newStatus: boolean) {
    if (this.terminated || this.running === newStatus) return

    this.running = newStatus

    if (newStatus) {
      console.debug(`Launching ${this.id}...`)
      this.startSession()
      this.started = performance.now()
      try {
        await this.daemon?.start()
      } catch (e) {
        await this.setHealth({
          result: 'failure',
          message: `${this.id} daemon failed to start: ${asError(e).message}`,
        })
      }
    } else {
      console.debug(`Stopping ${this.id}...`)
      await this.stopSession()
      // Dependency loss pauses the daemon without destroying its SubContainer.
      await this.daemon?.stop()
    }
  }

  private async stopSession() {
    if (!this.session) return
    this.probeEpoch += 1
    this.session.abort.abort()
    await this.session.done
    this.session = null
    this.resetReady()
    this.setDependentsReady(false)
  }

  private resetReady() {
    this.resolvedReady = false
    this.readyPromise = new Promise(
      resolve =>
        (this.resolveReady = () => {
          resolve()
          this.resolvedReady = true
        }),
    )
  }

  private startSession() {
    this.session?.abort.abort()
    this.probeEpoch += 1

    const abort = new AbortController()

    this.daemon?.onExit(success => {
      if (abort.signal.aborted) return
      if (success && this.ready === 'EXIT_SUCCESS') {
        void this.setHealth({ result: 'success', message: null })
      } else if (!success) {
        this.probeEpoch += 1
        const cause = this.daemon?.lastExitError?.message
        void this.setHealth({
          result: 'failure',
          message: cause
            ? `${this.id} daemon crashed: ${cause}`
            : `${this.id} daemon crashed`,
        })
      } else if (!this.daemon?.isOneshot()) {
        this.probeEpoch += 1
        void this.setHealth({
          result: 'failure',
          message: `${this.id} daemon exited`,
        })
      }
    })

    const done =
      this.ready === 'EXIT_SUCCESS'
        ? Promise.resolve()
        : this.runHealthCheckLoop(abort.signal)

    this.session = { abort, done }
  }

  private async runHealthCheckLoop(signal: AbortSignal): Promise<void> {
    if (this.ready === 'EXIT_SUCCESS') return
    const ready = this.ready
    const triggerFactory: TerminalAwareTrigger = ready.trigger ?? defaultTrigger
    const trigger = triggerFactory(() => ({
      lastResult: this._health.result,
      lastWaiting: 'waiting' in this._health && this._health.waiting,
    }))

    const next = () =>
      new Promise<IteratorResult<unknown, unknown>>((resolve, reject) => {
        const stop = () => resolve({ done: true, value: undefined })
        if (signal.aborted) return stop()
        signal.addEventListener('abort', stop, { once: true })
        trigger.next().then(
          res => {
            signal.removeEventListener('abort', stop)
            resolve(res)
          },
          err => {
            signal.removeEventListener('abort', stop)
            reject(err)
          },
        )
      })

    try {
      for (let res = await next(); !res.done; res = await next()) {
        const epoch = this.probeEpoch
        const response: HealthCheckResult = await Promise.resolve()
          .then(() => ready.fn())
          .catch(err => ({
            result: 'failure' as const,
            message: formatError(err),
          }))

        if (signal.aborted) break
        if (epoch !== this.probeEpoch) continue
        const terminal =
          triggerFactory[isTerminalResult]?.(response.result) ?? false
        await this.setHealth(response, { tolerateTransient: !terminal })
      }
    } catch (err) {
      if (!signal.aborted) {
        console.error(
          `Daemon ${this.id} health check failed: ${safeString(err)}`,
        )
      }
    }
    if (!signal.aborted && this.consecutiveUnhealthy > 0) {
      this.setDependentsReady(false)
    }
  }

  onReady() {
    return this.readyPromise
  }

  get isReady() {
    return this.resolvedReady
  }

  private notifyWatchers(kind: 'health' | 'dependents') {
    this.healthWatchers.forEach(entry => {
      if (entry.kind === kind) entry.fn()
    })
  }

  private setDependentsReady(ready: boolean) {
    this.consecutiveUnhealthy = 0
    if (this.dependentsReady === ready) return
    this.dependentsReady = ready
    this.notifyWatchers('dependents')
  }

  private async setHealth(
    health: HealthCheckResult,
    { tolerateTransient = false } = {},
  ) {
    const changed = this._health.result !== health.result
    this._health = health
    if (this.resolveReady && health.result === 'success') {
      this.resolveReady()
    }
    if (health.result === 'success') {
      this.setDependentsReady(true)
    } else if (tolerateTransient && this.dependentsReady) {
      this.consecutiveUnhealthy += 1
      if (this.consecutiveUnhealthy >= READY_FAILURE_THRESHOLD) {
        this.setDependentsReady(false)
      }
    } else {
      this.setDependentsReady(false)
    }
    if (changed) {
      this.notifyWatchers('health')
    }
    if (this.ready === 'EXIT_SUCCESS') return
    const displayedHealth = withGracePeriod(
      health,
      this.started,
      this.ready.gracePeriod,
    )
    if (displayedHealth.result === 'failure') {
      console.error(`Health Check ${this.id} failed:`, health.message)
    }
    const display = this.ready.display
    if (!display) {
      return
    }
    try {
      await this.effects.setHealth({
        ...displayedHealth,
        id: this.id,
        name: display,
      })
    } catch (err) {
      console.error(`Failed to report health for ${this.id}:`, err)
    }
  }

  private waitingOnDependencies() {
    return this.dependencies
      .filter(d =>
        this.strictDependencies
          ? !(d.running && d._health.result === 'success')
          : !(d.running && d.dependentsReady),
      )
      .map(d => ({
        id: d.id,
        display: typeof d.ready === 'object' ? d.ready.display : null,
      }))
  }

  async updateStatus() {
    if (this.terminated) return
    const waitingOn = this.waitingOnDependencies()
    if (waitingOn.length) {
      this.probeEpoch += 1
      console.debug(`daemon ${this.id} waiting on ${waitingOn.map(w => w.id)}`)
      const waitingOnNames = waitingOn.flatMap(w =>
        w.display ? [w.display] : [],
      )
      const message = waitingOnNames.length ? waitingOnNames.join(', ') : null
      const pub = this.setHealth({ result: 'starting', waiting: true, message })
      const run = this.changeRunning(false)
      await Promise.all([pub, run])
    } else {
      await this.setHealth({ result: 'starting', message: null })
      await this.changeRunning(!this.waitingOnDependencies().length)
    }
  }
}
