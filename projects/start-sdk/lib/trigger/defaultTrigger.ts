import { HealthStatus } from '@start9labs/start-core/types'
import { Trigger } from '.'
import { delay, ExponentialBackoff } from '../util/delay'

const SETTLED_MS = 30_000
const PENDING: { [K in HealthStatus]?: number } = {
  starting: 1_000,
  failure: 1_000,
}

/**
 * The trigger used when no explicit `trigger` is set on a health check.
 *
 * Polls every 1 s while `starting` or `failure`, and every 30 s while
 * `success`, `loading`, `disabled`, or `starting` with `waiting: true`,
 * except that `loading` or `waiting: true` before the first `success` backs
 * off from 1 s, doubling to 30 s.
 */
export const defaultTrigger: Trigger = async function* (getInput) {
  let succeeded = false
  const loading = new ExponentialBackoff(1_000, SETTLED_MS)
  while (true) {
    const input = getInput()
    succeeded ||= input.lastResult === 'success'
    const backoff =
      !succeeded && (input.lastResult === 'loading' || input.lastWaiting)
    if (!backoff) loading.reset()
    const ms = backoff
      ? loading.next()
      : input.lastWaiting
        ? SETTLED_MS
        : (PENDING[input.lastResult ?? 'starting'] ?? SETTLED_MS)
    await delay(ms)
    yield
  }
}
