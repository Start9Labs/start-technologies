import { T } from '@start9labs/start-core'
import { TuiDialogContext } from '@taiga-ui/core'
import { Result } from 'src/app/services/api/api.types'

export type BackupContext = TuiDialogContext<void, { id: string }>
export type BackupRun = T.BackupRun
export type BackupHistory = Result<'backup.history.list'>
export const EMPTY_HISTORY: BackupHistory = { runs: [], total: 0 }

export type RecoverOption =
  | (T.PackageBackupInfo & {
      kind: 'package'
      id: string
      installed: boolean
      newerOs: boolean
    })
  | (Pick<T.SharedVolumeBackupInfo, 'timestamp'> & {
      kind: 'volume'
      id: string
      title: string
    })

export interface RecoverData {
  targetId: string
  serverId: string
  backupInfo: T.BackupInfo
  password: string
}
