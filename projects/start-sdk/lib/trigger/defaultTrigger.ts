import { HealthStatus } from '@start9labs/start-core/types'
import { Trigger } from '.'
import { TriggerInput } from './TriggerInput'
import { delay } from './delay'

const SETTLED_MS = 30_000
const PENDING: { [K in HealthStatus]?: number } = {
  starting: 1_000,
  failure: 1_000,
}

function defaultInterval({ lastResult, lastWaiting }: TriggerInput) {
  return lastWaiting
    ? SETTLED_MS
    : (PENDING[lastResult ?? 'starting'] ?? SETTLED_MS)
}

/**
 * The trigger used when no explicit `trigger` is set on a health check.
 *
 * Polls every 1 s while `starting` or `failure`, and every 30 s while
 * `success`, `loading`, `disabled`, or `starting` with `waiting: true`.
 */
export const defaultTrigger: Trigger = async function* (getInput) {
  while (true) {
    await delay(defaultInterval(getInput()))
    yield
  }
}

/**
 * The trigger for a daemon's `ready` check when none is set: as
 * {@link defaultTrigger}, except that `loading` or `waiting: true` before the
 * first `success` backs off from 1 s, doubling to 30 s.
 */
export const daemonReadyTrigger: Trigger = async function* (getInput) {
  let succeeded = false
  let loadingMs = 1_000
  while (true) {
    const input = getInput()
    succeeded ||= input.lastResult === 'success'
    const backoff =
      !succeeded && (input.lastResult === 'loading' || input.lastWaiting)
    const ms = backoff ? loadingMs : defaultInterval(input)
    loadingMs = backoff ? Math.min(loadingMs * 2, SETTLED_MS) : 1_000
    await delay(ms)
    yield
  }
}
