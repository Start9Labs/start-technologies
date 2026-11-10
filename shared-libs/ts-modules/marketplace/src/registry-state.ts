import type { T } from '@start9labs/start-core'

/** Missing observations are still awaiting their first synchronization attempt. */
export function registryStatus(
  cache?: { status?: T.RegistryStatus } | null,
): T.RegistryStatus {
  return cache?.status ?? 'connecting'
}

export function isRegistryLoading(
  status: T.RegistryStatus | undefined,
  packageCount: number,
): boolean {
  return status === 'connecting' && packageCount === 0
}
