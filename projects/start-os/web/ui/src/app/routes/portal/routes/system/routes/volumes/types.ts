import { i18nKey } from '@start9labs/shared'
import { T } from '@start9labs/start-core'

export const VOLUME_TYPES: Record<
  T.SharedVolumeSource['kind'],
  { icon: string; label: i18nKey }
> = {
  local: { icon: '@tui.folder', label: 'Local folder' },
  disk: { icon: '@tui.hard-drive', label: 'External drive' },
  cifs: { icon: '@tui.network', label: 'CIFS share' },
}

export const isStale = (checkedAt: string | null) =>
  !checkedAt || Date.now() - Date.parse(checkedAt) > 10 * 60 * 1000

export const VOLUME_STATUSES: Record<T.SharedVolumeAvailability, i18nKey> = {
  unknown: 'Unknown',
  available: 'Available',
  unavailable: 'Unavailable',
}

export const volumePath = (source: T.SharedVolumeSource) =>
  source.kind === 'local' ? '/' : `/${source.path.replace(/^\//, '')}`

/** CIFS display names follow the hostname, independent of stored aliases. */
export const volumeName = ({
  name,
  source,
}: Pick<T.SharedVolume, 'name' | 'source'>) =>
  source.kind === 'cifs' ? source.hostname : name
