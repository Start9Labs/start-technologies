export function isInGracePeriod(started: number, gracePeriod = 10_000) {
  return performance.now() - started < gracePeriod
}
