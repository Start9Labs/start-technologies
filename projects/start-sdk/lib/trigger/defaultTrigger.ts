import { HealthStatus } from '@start9labs/start-core/types'
import { Trigger } from '.'
import { statusTrigger } from './statusTrigger'
import { delay } from './delay'

const SETTLED_MS = 30_000
const PENDING: { [K in HealthStatus]?: number } = {
  starting: 1_000,
  failure: 1_000,
}

/**
 * The trigger used when no explicit `trigger` is set on a health check.
 *
 * Polls every 1 s while `starting` or `failure`, and every 30 s while
 * `success`, `loading`, `waiting` or `disabled`.
 */
export const defaultTrigger = statusTrigger(SETTLED_MS, PENDING)

/**
 * The trigger for a daemon's `ready` check when none is set: as
 * {@link defaultTrigger}, except that `loading` or `waiting` before the first
 * `success` backs off from 1 s, doubling to 30 s.
 */
export const daemonReadyTrigger: Trigger = async function* (getInput) {
  let succeeded = false
  let loadingMs = 1_000
  while (true) {
    const { lastResult } = getInput()
    succeeded ||= lastResult === 'success'
    const backoff =
      !succeeded && (lastResult === 'loading' || lastResult === 'waiting')
    const ms = backoff
      ? loadingMs
      : (PENDING[lastResult ?? 'starting'] ?? SETTLED_MS)
    loadingMs = backoff ? Math.min(loadingMs * 2, SETTLED_MS) : 1_000
    await delay(ms)
    yield
  }
}
