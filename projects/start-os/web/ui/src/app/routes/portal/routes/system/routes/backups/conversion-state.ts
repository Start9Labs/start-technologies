import type { i18nKey } from '@start9labs/shared'
import type { T } from '@start9labs/start-core'

export function serverBackups<T>(
  backups: Record<string, T>,
  serverId: string,
): Record<string, T> {
  return Object.fromEntries(
    Object.entries(backups).filter(([id]) => id === serverId),
  )
}

export function conversionLabel(
  backups: Record<string, T.StartOsRecoveryInfo>,
  jobs: Record<string, T.ConversionJob> = {},
  serverId: string,
): i18nKey | null {
  backups = serverBackups(backups, serverId)
  const visibleJobs = Object.keys(backups).map(id => jobs[id])
  if (visibleJobs.some(job => job?.state === 'running'))
    return 'Conversion in progress'
  if (
    Object.values(backups).some(backup => backup.formatStatus === 'interrupted')
  )
    return 'Resume conversion'
  if (visibleJobs.some(job => job?.state === 'failed'))
    return 'Retry conversion'
  if (Object.values(backups).some(backup => backup.formatStatus === 'legacy'))
    return 'Update format'
  return null
}
