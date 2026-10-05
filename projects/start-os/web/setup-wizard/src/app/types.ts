import { DiskInfo, PartitionInfo, StartOSDiskInfo } from '@start9labs/shared'

// === Install OS === (no binding available)

export interface InstallOsParams {
  osDrive: string | null // null = already booted from the OS drive; provision data only
  dataDrive: {
    stablePath: string
    wipe: boolean
  }
}

export interface InstallOsRes {
  guid: string // data drive guid
  attach: boolean
  mokEnrolled: boolean
}

// === Disk Info Helpers ===

export type StartOSDiskInfoWithId = StartOSDiskInfo & {
  id: string
}

export type StartOSDiskInfoFull = StartOSDiskInfoWithId & {
  partition: PartitionInfo
  drive: DiskInfo
}
