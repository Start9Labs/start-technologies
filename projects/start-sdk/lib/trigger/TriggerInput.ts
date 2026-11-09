import { HealthStatus } from '@start9labs/start-core/types'

/**
 * State provided to trigger functions on each iteration.
 *
 * Triggers can inspect `lastResult` to adapt their polling interval based
 * on the most recent health check outcome.
 */
export type TriggerInput = {
  /** The result of the most recent health check invocation, or `undefined` if it hasn't run yet */
  lastResult?: HealthStatus
  /** Whether the most recent `starting` or `loading` result is waiting. */
  lastWaiting?: boolean
}
