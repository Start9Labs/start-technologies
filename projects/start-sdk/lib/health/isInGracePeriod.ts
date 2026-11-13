import { HealthCheckResult } from './checkFns/HealthCheckResult'

export function withGracePeriod(
  health: HealthCheckResult,
  started: number | undefined,
  gracePeriod?: number,
): HealthCheckResult {
  return health.result === 'failure' &&
    started !== undefined &&
    isInGracePeriod(started, gracePeriod)
    ? { result: 'starting', message: null }
    : health
}

export function isInGracePeriod(started: number, gracePeriod = 10_000) {
  return performance.now() - started < gracePeriod
}
