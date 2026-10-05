export type DiskListResponse = DiskInfo[]

export interface DiskInfo {
  logicalname: string
  stablePath: string
  vendor: string | null
  model: string | null
  partitions: PartitionInfo[]
  capacity: number
  guid: string | null
  filesystem: string | null
}

export interface PartitionInfo {
  logicalname: string
  stablePath: string
  label: string | null
  capacity: number
  used: number | null
  startOs: Record<string, StartOSDiskInfo>
  guid: string | null
  filesystem: string | null
}

export type StartOSDiskInfo = {
  hostname: string
  version: string
  timestamp: string
}
