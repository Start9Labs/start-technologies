import { HealthStatus } from '@start9labs/start-core/types'
import { Trigger } from '.'
import { delay } from './delay'

type StatusIntervals = { [K in HealthStatus]?: number }

export const isTerminalResult = Symbol('isTerminalResult')

export type TerminalAwareTrigger = Trigger & {
  [isTerminalResult]?: (result: HealthStatus) => boolean
}

/**
 * A trigger with per-status polling intervals.
 *
 * Map each health check result to a cooldown duration in milliseconds.
 * Any status without an explicit interval uses the default (30 s if omitted).
 * An interval of `Infinity` stops polling once that status is reported.
 *
 * @param defaultMs - Fallback interval (ms) for any status not listed. Defaults to 30 s.
 * @param intervals - Per-status overrides
 *
 * @example
 * ```ts
 * trigger: sdk.trigger.statusTrigger(30_000, {
 *   starting: 5_000,
 *   failure: 5_000,
 * })
 * ```
 */
export function statusTrigger(
  defaultMs: number,
  intervals: StatusIntervals = {},
): Trigger {
  const trigger: TerminalAwareTrigger = async function* (getInput) {
    while (true) {
      const { lastResult } = getInput()
      const ms = lastResult
        ? (intervals[lastResult] ?? defaultMs)
        : (intervals.starting ?? defaultMs)
      await delay(ms)
      yield
    }
  }
  trigger[isTerminalResult] = result =>
    (intervals[result] ?? defaultMs) === Infinity
  return trigger
}
