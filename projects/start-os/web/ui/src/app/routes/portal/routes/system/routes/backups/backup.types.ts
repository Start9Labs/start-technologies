import { T } from '@start9labs/start-core'
import { TuiDialogContext } from '@taiga-ui/core'

export type BackupContext = TuiDialogContext<void, { id: string }>
export type BackupRun = T.BackupRun

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
