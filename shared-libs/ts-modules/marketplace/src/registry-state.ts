import type { T } from '@start9labs/start-core'

/** Missing observations are still awaiting their first synchronization attempt. */
export function registryStatus(
  cache?: { status?: T.RegistryStatus } | null,
): T.RegistryStatus {
  return cache?.status ?? 'connecting'
}

/** An unavailable catalog must not appear to be a successful empty listing. */
export function isRegistryLoading(
  status: T.RegistryStatus | undefined,
  packageCount: number,
): boolean {
  return (status === 'connecting' || status === 'error') && packageCount === 0
}
