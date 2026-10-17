import { Trigger } from '.'

/**
 * A trigger that runs the health check once, as soon as it starts.
 *
 * For a check whose result cannot change until `main` restarts, such as one
 * computed from configuration read with `.const()`. Pair it with
 * `gracePeriod: 0`, or the single result is reported as `starting`.
 *
 * @example
 * ```ts
 * ready: {
 *   display: 'Signups',
 *   gracePeriod: 0,
 *   trigger: sdk.trigger.onceTrigger,
 *   fn: () => (signupsOpen ? { result: 'failure', message } : { result: 'success', message: null }),
 * }
 * ```
 */
export const onceTrigger: Trigger = async function* () {
  yield
}
