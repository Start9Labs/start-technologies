import { IST, RPC, T } from '@start9labs/start-core'
import { WebSocketSubjectConfig } from 'rxjs/webSocket'
import { GetPackageReq, GetPackagesReq } from '@start9labs/marketplace'

export type Api = RPC.StartOS | RPC.Diagnostic | RPC.Init
export type Params<M extends RPC.RpcMethod<Api>> = RPC.RpcParamType<Api, M>
export type Result<M extends RPC.RpcMethod<Api>> = RPC.RpcReturnType<Api, M>
export type WebsocketConfig<U> = Omit<WebSocketSubjectConfig<U>, 'url'>
export type ServerState = Result<'state'>
export type DiagnosticErrorRes = Result<'diagnostic.error'>
export type FollowServerLogsReq = Params<'server.logs.follow'>
export type ServerBindingSetAddressEnabledReq =
  Params<'server.host.binding.set-address-enabled'>
export type PkgBindingSetAddressEnabledReq =
  Params<'package.host.binding.set-address-enabled'>
export type ServerBindingSetGuaWanReq =
  Params<'server.host.binding.set-gua-wan'>
export type PkgBindingSetGuaWanReq = Params<'package.host.binding.set-gua-wan'>
export type PkgAddPublicDomainReq =
  Params<'package.host.address.domain.public.add'>
export type PkgRemovePublicDomainReq =
  Params<'package.host.address.domain.public.remove'>
export type PkgAddPrivateDomainReq =
  Params<'package.host.address.domain.private.add'>
export type PkgRemovePrivateDomainReq =
  Params<'package.host.address.domain.private.remove'>
export type GetPackageLogsReq = Params<'package.logs'>
export type FollowPackageLogsReq = Params<'package.logs.follow'>

export type GetActionInputRes =
  | (Omit<NonNullable<Result<'package.action.get-input'>>, 'spec'> & {
      spec: IST.InputSpec
    })
  | null
export type ActionRes = Extract<
  NonNullable<Result<'package.action.run'>>,
  { version: '1' }
> | null
export type GetRegistryPackageReq = GetPackageReq &
  Pick<Params<'registry.package.get'>, 'registry'>
export type GetRegistryPackagesReq = GetPackagesReq &
  Pick<Params<'registry.package.get'>, 'registry'>
export type CheckDnsRes = Result<'net.gateway.check-dns'>
export type DiskBackupTarget = Extract<T.BackupTarget, { type: 'disk' }>
export type CifsBackupTarget = Extract<T.BackupTarget, { type: 'cifs' }>
export type RecoverySource = DiskRecoverySource | CifsRecoverySource
export interface DiskRecoverySource {
  type: 'disk'
  logicalname: string
}
export interface CifsRecoverySource {
  type: 'cifs'
  hostname: string
  path: string
  username: string
  password: string
}
export type ServerNotification<N extends number> = Omit<
  T.NotificationWithId,
  'code' | 'data'
> & {
  code: N
  data: NotificationData<N>
}
export type NotificationData<N> = N extends 0
  ? null
  : N extends 1
    ? T.BackupReport
    : N extends 2
      ? string
      : unknown

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
