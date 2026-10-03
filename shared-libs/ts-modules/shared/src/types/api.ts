import type { RPC } from '@start9labs/start-core'

export type DiskListResponse = RPC.RpcReturnType<RPC.Setup, 'setup.disk.list'>
export type DiskInfo = DiskListResponse[number]
export type PartitionInfo = DiskInfo['partitions'][number]
export type StartOSDiskInfo = PartitionInfo['startOs'][string]
