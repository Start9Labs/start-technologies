import { DiskInfo, PartitionInfo, StartOSDiskInfo } from '@start9labs/shared'

import { Params, Result } from './services/api.service'

export type InstallOsParams = Params<'setup.install-os'>
export type InstallOsRes = Result<'setup.install-os'>

export type StartOSDiskInfoWithId = StartOSDiskInfo & {
  id: string
}

export type StartOSDiskInfoFull = StartOSDiskInfoWithId & {
  partition: PartitionInfo
  drive: DiskInfo
}
