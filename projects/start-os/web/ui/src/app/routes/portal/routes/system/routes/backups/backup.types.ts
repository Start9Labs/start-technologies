import { T } from '@start9labs/start-core'
import { TuiDialogContext } from '@taiga-ui/core'
import { Source } from 'src/app/routes/portal/routes/system/routes/volumes/create'
import { SourceClient } from 'src/app/routes/portal/routes/system/routes/volumes/source-fields'
import { ApiService } from 'src/app/services/api/embassy-api.service'

export type BackupContext = TuiDialogContext<void, { id: string }>

export type BackupTargetClient = SourceClient & {
  create: (params: { name: string; source: Source }) => Promise<string>
  remove: (params: { id: string }) => Promise<null>
  inspect: (params: { id: string }) => Promise<unknown>
}

export const targetRpc = (api: ApiService) =>
  api.rpc.backup.target as unknown as BackupTargetClient

export type BackupRun = {
  id: string
  targetId: string
  targetName: string
  startedAt: string
  completedAt: string
  report: T.BackupReport
}

export type BackupHistory = { runs: BackupRun[]; total: number }

export const EMPTY_HISTORY: BackupHistory = { runs: [], total: 0 }

export const historyRpc = (api: ApiService) =>
  (
    api.rpc.backup as unknown as {
      history: {
        list: (params: {
          offset: number
          limit: number
        }) => Promise<BackupHistory>
      }
    }
  ).history

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
