import { T } from '@start9labs/start-core'

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

export interface CheckpointRecoverOption {
  id: string
  title: string
  checked: boolean
  installed: boolean
  newerOs: boolean
  selectedKey: string
  checkpoints: RecoverCheckpoint[]
}

export interface CheckpointRecoverData {
  targetId: string
  serverId: string
  backupInfo: T.BackupInfo
  scheduledHistories: T.ServiceTargetHistory[]
  password: string
  unavailableSources?: string[]
}
