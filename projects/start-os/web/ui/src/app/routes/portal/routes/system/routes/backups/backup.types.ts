import { T } from '@start9labs/start-core'
import { TuiDialogContext } from '@taiga-ui/core'
import {
  CifsBackupTarget,
  DiskBackupTarget,
} from 'src/app/services/api/api.types'
import { MappedBackupTarget } from './backup.service'

export type BackupContext = TuiDialogContext<
  void,
  MappedBackupTarget<CifsBackupTarget | DiskBackupTarget>
>

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
