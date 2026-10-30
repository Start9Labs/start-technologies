import type { i18nKey } from '@start9labs/shared'
import type { T } from '@start9labs/start-core'

export function conversionLabel(
  backups: Record<string, T.StartOsRecoveryInfo>,
  jobs: Record<string, T.ConversionJob> = {},
): i18nKey | null {
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
    return 'Old format'
  return null
}
