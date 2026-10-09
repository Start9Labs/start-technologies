import { IST, T } from '@start9labs/start-core'
import { WebSocketSubjectConfig } from 'rxjs/webSocket'
import { GetPackageReq, GetPackagesReq } from '@start9labs/marketplace'

// websocket

export type WebsocketConfig<U> = Omit<WebSocketSubjectConfig<U>, 'url'>

// state

export type ServerState = 'initializing' | 'error' | 'running'

// diagnostic

export type DiagnosticErrorRes = {
  code: number
  message: string
  data: { details: string }
}

// logs

export type FollowServerLogsReq = Omit<T.LogsParams, 'before'>

// bindings

export type ServerBindingSetAddressEnabledReq = {
  // server.host.binding.set-address-enabled
  internalPort: 80
  address: T.HostnameInfo
  enabled: boolean | null // null = reset to default
}

export type PkgBindingSetAddressEnabledReq = Omit<
  ServerBindingSetAddressEnabledReq,
  'internalPort'
> & {
  // package.host.binding.set-address-enabled
  internalPort: number
  package: T.PackageId // string
  host: T.HostId // string
}

export type ServerBindingSetGuaWanReq = {
  // server.host.binding.set-gua-wan
  internalPort: 80
  address: T.HostnameInfo
  wan: boolean
}

export type PkgBindingSetGuaWanReq = Omit<
  ServerBindingSetGuaWanReq,
  'internalPort'
> & {
  // package.host.binding.set-gua-wan
  internalPort: number
  package: T.PackageId // string
  host: T.HostId // string
}

// package domains

export type PkgAddPublicDomainReq = T.AddPublicDomainParams & {
  // package.host.address.domain.public.add
  package: T.PackageId // string
  host: T.HostId // string
}

export type PkgRemovePublicDomainReq = T.RemoveDomainParams & {
  // package.host.address.domain.public.remove
  package: T.PackageId // string
  host: T.HostId // string
}

export type PkgAddPrivateDomainReq = T.AddPrivateDomainParams & {
  // package.host.address.domain.private.add
  package: T.PackageId // string
  host: T.HostId // string
}

export type PkgRemovePrivateDomainReq = T.RemoveDomainParams & {
  // package.host.address.domain.private.remove
  package: T.PackageId // string
  host: T.HostId // string
}

// package logs

export type GetPackageLogsReq = T.LogsParams & { id: string } // package.logs

export type FollowPackageLogsReq = FollowServerLogsReq & { id: string } // package.logs.follow

// actions

export type GetActionInputRes = {
  eventId: string
  spec: IST.InputSpec
  value: object | null
}

export type ActionRes = (T.ActionResult & { version: '1' }) | null

// registry

export type GetRegistryPackageReq = GetPackageReq & { registry: string }

export type GetRegistryPackagesReq = GetPackagesReq & { registry: string }

// dns
// TODO: Replace with T.CheckDnsRes when SDK types are generated
export type CheckDnsRes = boolean

// backup

export type DiskBackupTarget = Extract<T.BackupTarget, { type: 'disk' }>
export type CifsBackupTarget = T.CifsBackupTarget & { type: 'cifs' }

export type RecoverySource = DiskRecoverySource | CifsRecoverySource

export interface DiskRecoverySource {
  type: 'disk'
  logicalname: string // partition logicalname
}

export interface CifsRecoverySource {
  type: 'cifs'
  hostname: string
  path: string
  username: string
  password: string
}

// notifications

export type ServerNotification<N extends number> = {
  id: number
  packageId: string | null
  createdAt: string
  code: N
  level: T.NotificationLevel
  title: string
  message: string
  data: NotificationData<N>
  seen: boolean
}

export type NotificationData<N> = N extends 0
  ? null
  : N extends 1
    ? T.BackupReport
    : N extends 2
      ? string
      : any

declare global {
  type Stringified<T> = string & {
    [P in keyof T]: T[P]
  }

  interface JSON {
    stringify<T>(
      value: T,
      replacer?: (key: string, value: any) => any,
      space?: string | number,
    ): string & Stringified<T>
    parse<T>(text: Stringified<T>, reviver?: (key: any, value: any) => any): T
  }
}

// @TODO 041

// export namespace RR041 {

//   // ** automated backups **

//   export type GetBackupTargetsReq = {} // backup.targets
//   export type GetBackupTargetsRes = {
//     unknownDisks: UnknownDisk[]
//     saved: Record<string, BackupTarget>
//   }

//   export type AddCifsBackupTargetReq = {
//     name: string
//     path: string
//     hostname: string
//     username: string
//     password?: string
//   } // backup.target.cifs.add
//   export type AddCloudBackupTargetReq = {
//     name: string
//     path: string
//     provider: CloudProvider
//     [params: string]: any
//   } // backup.target.cloud.add
//   export type AddDiskBackupTargetReq = {
//     logicalname: string
//     name: string
//     path: string
//   } // backup.target.disk.add
//   export type AddBackupTargetRes = Record<string, BackupTarget>

//   export type UpdateCifsBackupTargetReq = AddCifsBackupTargetReq & {
//     id: string
//   } // backup.target.cifs.update
//   export type UpdateCloudBackupTargetReq = AddCloudBackupTargetReq & {
//     id: string
//   } // backup.target.cloud.update
//   export type UpdateDiskBackupTargetReq = Omit<
//     AddDiskBackupTargetReq,
//     'logicalname'
//   > & {
//     id: string
//   } // backup.target.disk.update
//   export type UpdateBackupTargetRes = AddBackupTargetRes

//   export type RemoveBackupTargetReq = { id: string } // backup.target.remove
//   export type RemoveBackupTargetRes = null

//   export type GetBackupJobsReq = {} // backup.job.list
//   export type GetBackupJobsRes = BackupJob[]

//   export type CreateBackupJobReq = {
//     name: string
//     targetId: string
//     cron: string
//     packageIds: string[]
//     now: boolean
//   } // backup.job.create
//   export type CreateBackupJobRes = BackupJob

//   export type UpdateBackupJobReq = Omit<CreateBackupJobReq, 'now'> & {
//     id: string
//   } // backup.job.update
//   export type UpdateBackupJobRes = CreateBackupJobRes

//   export type DeleteBackupJobReq = { id: string } // backup.job.delete
//   export type DeleteBackupJobRes = null

//   export type GetBackupRunsReq = {} // backup.runs
//   export type GetBackupRunsRes = BackupRun[]

//   export type DeleteBackupRunsReq = { ids: string[] } // backup.runs.delete
//   export type DeleteBackupRunsRes = null

//   export type GetBackupInfoReq = { targetId: string; password: string } // backup.target.info
//   export type GetBackupInfoRes = BackupInfo

//   export type CreateBackupReq = { targetId: string; packageIds: string[] } // backup.create
//   export type CreateBackupRes = null
// }

// @TODO 041 types

// export type RemoteBackupTarget = CifsBackupTarget | CloudBackupTarget
// export type BackupTarget = RemoteBackupTarget | DiskBackupTarget

// export type BackupTargetType = 'disk' | 'cifs' | 'cloud'

// export interface UnknownDisk {
//   logicalname: string
//   vendor: string | null
//   model: string | null
//   label: string | null
//   capacity: number
//   used: number | null
//   startOs: Record<string, StartOSDiskInfo>
// }

// export interface BaseBackupTarget {
//   type: BackupTargetType
//   name: string
//   mountable: boolean
//   path: string
//   startOs: Record<string, StartOSDiskInfo>
// }

// export interface DiskBackupTarget extends UnknownDisk, BaseBackupTarget {
//   type: 'disk'
// }

// export interface CifsBackupTarget extends BaseBackupTarget {
//   type: 'cifs'
//   hostname: string
//   username: string
// }

// export interface CloudBackupTarget extends BaseBackupTarget {
//   type: 'cloud'
//   provider: 'dropbox' | 'google-drive'
// }

// export type BackupRun = {
//   id: string
//   startedAt: string
//   completedAt: string
//   packageIds: string[]
//   job: BackupJob
//   report: BackupReport
// }

// export type BackupJob = {
//   id: string
//   name: string
//   targetId: string
//   cron: string // '* * * * * *' https://cloud.google.com/scheduler/docs/configuring/cron-job-schedules
//   packageIds: string[]
// }

// export type CloudProvider = 'dropbox' | 'google-drive'
