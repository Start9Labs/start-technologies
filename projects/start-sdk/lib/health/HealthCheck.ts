import { Effects, HealthCheckId } from '@start9labs/start-core/types'
import { HealthCheckResult } from './checkFns/HealthCheckResult'
import { Trigger } from '../trigger'
import { TriggerInput } from '../trigger/TriggerInput'
import { defaultTrigger } from '../trigger/defaultTrigger'
import { Drop } from '@start9labs/start-core/util/Drop'
import { isInGracePeriod, withGracePeriod } from './isInGracePeriod'

/** Parameters for creating a health check */
export type HealthCheckParams = {
  id: HealthCheckId
  name: string
  trigger?: Trigger
  gracePeriod?: number
  fn(): Promise<HealthCheckResult> | HealthCheckResult
}

/**
 * A periodic health check that reports daemon readiness to the StartOS UI.
 *
 * Polls at an interval controlled by a {@link Trigger}, reporting results as
 * "starting" (during the grace period), "success", or "failure". Automatically
 * pauses when the daemon is stopped and resumes when restarted.
 */
export class HealthCheck extends Drop {
  private trigger: ReturnType<Trigger> | null = null
  private started: number | null = null
  private setStarted = (started: number | null) => {
    this.started = started
  }
  private exited = false
  private exit = () => {
    this.exited = true
  }
  private currentValue: TriggerInput = {}
  private promise: Promise<void>
  private constructor(
    effects: Effects,
    private readonly o: HealthCheckParams,
  ) {
    super()
    this.promise = Promise.resolve().then(async () => {
      const checkStarted = () =>
        [
          this.started,
          new Promise<void>(resolve => {
            this.setStarted = (started: number | null) => {
              this.started = started
              resolve()
            }
            this.exit = () => {
              this.exited = true
              resolve()
            }
          }),
        ] as const
      let triggered: ReturnType<Trigger> | null = null
      while (!this.exited) {
        const [started, changed] = checkStarted()
        const trigger = this.trigger
        let race:
          | [Promise<void>]
          | [Promise<void>, Promise<IteratorResult<unknown, unknown>>] = [
          changed,
        ]
        if (started !== null && trigger) {
          race = [...race, trigger.next()]
          if (triggered === trigger) {
            try {
              const health = await o.fn()
              const displayedHealth = withGracePeriod(
                health,
                started,
                o.gracePeriod,
              )
              await effects.setHealth({
                ...displayedHealth,
                name: o.name,
                id: o.id,
              })
              this.currentValue = {
                lastResult: displayedHealth.result,
                lastWaiting:
                  'waiting' in displayedHealth && displayedHealth.waiting,
              }
            } catch (e) {
              await effects.setHealth({
                name: o.name,
                id: o.id,
                result: isInGracePeriod(started, o.gracePeriod)
                  ? 'starting'
                  : 'failure',
                message: asMessage(e) || '',
              })
              this.currentValue = { lastResult: 'failure' }
            }
          }
        } else triggered = null
        const raced = await Promise.race(race)
        if (raced) {
          if (raced.done) await changed
          else triggered = trigger
        }
      }
    })
  }
  /**
   * Create a new HealthCheck instance and begin its polling loop.
   * @param effects - The effects context for reporting health status
   * @param options - Health check configuration (ID, name, check function, trigger, grace period)
   * @returns A new HealthCheck instance
   */
  static of(effects: Effects, options: HealthCheckParams): HealthCheck {
    return new HealthCheck(effects, options)
  }
  /** Signal that the daemon is running, enabling health check polling */
  start() {
    if (this.started !== null) return
    this.trigger = (this.o.trigger ?? defaultTrigger)(() => this.currentValue)
    this.setStarted(performance.now())
  }
  /** Signal that the daemon has stopped, pausing health check polling */
  stop() {
    if (this.started === null) return
    this.setStarted(null)
  }
  onDrop(): void {
    this.exit()
  }
}

function asMessage(e: unknown) {
  if (typeof e === 'object' && e !== null && 'message' in e)
    return String((e as any).message)
  const value = String(e)
  return value
}
