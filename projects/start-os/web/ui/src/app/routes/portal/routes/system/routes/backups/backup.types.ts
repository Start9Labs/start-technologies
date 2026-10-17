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

export interface RecoverCheckpoint {
  key: string
  source: 'manual' | 'scheduled'
  version: string
  timestamp: string
  jobName?: string
  snapshotId?: string
  runId?: string
  archived?: boolean
}

export interface RecoverOption {
  id: string
  title: string
  checked: boolean
  installed: boolean
  newerOs: boolean
  selectedKey: string
  checkpoints: RecoverCheckpoint[]
}

export interface RecoverData {
  targetId: string
  serverId: string
  backupInfo: T.BackupInfo
  scheduledHistories: T.ServiceTargetHistory[]
  password: string
  unavailableSources?: string[]
}
