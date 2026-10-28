import type { z } from 'zod'
import type * as schemas from './tunnel.schemas'
export type RpcHandler = {
  _PARAMS: unknown
  _RETURN?: unknown
  _CHILDREN?: { [name: string]: RpcHandler }
}

export type RpcMethod<Root extends RpcHandler> =
  | (Root extends { _RETURN: unknown } ? '' : never)
  | (Root extends { _CHILDREN: infer Children }
      ? {
          [Name in keyof Children & string]: Children[Name] extends RpcHandler
            ? RpcMethod<Children[Name]> extends infer ChildMethod extends string
              ? ChildMethod extends ''
                ? Name
                : `${Name}.${ChildMethod}`
              : never
            : never
        }[keyof Children & string]
      : never)

export type RpcParamType<
  Root extends RpcHandler,
  Method extends string,
> = Method extends ''
  ? Root extends { _RETURN: unknown }
    ? Root['_PARAMS']
    : never
  : Root extends { _CHILDREN: infer Children }
    ? Method extends `${infer Head}.${infer Tail}`
      ? Head extends keyof Children
        ? Children[Head] extends RpcHandler
          ? RpcParamType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? RpcParamType<Children[Method], ''>
          : never
        : never
    : never

export type RpcReturnType<
  Root extends RpcHandler,
  Method extends string,
> = Method extends ''
  ? Root extends { _RETURN: infer Return }
    ? Return
    : never
  : Root extends { _CHILDREN: infer Children }
    ? Method extends `${infer Head}.${infer Tail}`
      ? Head extends keyof Children
        ? Children[Head] extends RpcHandler
          ? RpcReturnType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? RpcReturnType<Children[Method], ''>
          : never
        : never
    : never

export type RpcSchemaTree<Schema> = {
  [Key in keyof Schema as Key extends '_OPAQUE'
    ? never
    : Key]: Key extends '_PARAMS'
    ? Schema[Key] extends z.ZodType
      ? z.input<Schema[Key]>
      : never
    : Key extends '_RETURN'
      ? Schema[Key] extends z.ZodType
        ? z.output<Schema[Key]>
        : never
      : Key extends '_CHILDREN'
        ? { [Name in keyof Schema[Key]]: RpcSchemaTree<Schema[Key][Name]> }
        : never
}
export type AddDnsRecordParamsInput = z.input<typeof schemas.AddDnsRecordParams>
export type AddPinholeParamsInput = z.input<typeof schemas.AddPinholeParams>
export type AddPortForwardParamsInput = z.input<
  typeof schemas.AddPortForwardParams
>
export type AddSubnetParamsInput = z.input<typeof schemas.AddSubnetParams>
export type AnyVerifyingKeyInput = z.input<typeof schemas.AnyVerifyingKey>
export type ApplyParamsInput = z.input<typeof schemas.ApplyParams>

/**
 * Per-subnet DNS proxy configuration. WireGuard clients on a subnet are pointed
 * at the subnet's in-tunnel server address (`.1`) for DNS; the forward-only proxy
 * listening there resolves every query against the upstream(s) selected here.
 */
export type DnsConfig = z.output<typeof schemas.DnsConfig>

/**
 * Which upstream a subnet's DNS proxy forwards to. `Device`/`Custom` draw
 * their data from companion fields on [`SetSubnetDnsParams`].
 */
export type DnsModeInput = z.input<typeof schemas.DnsMode>

/**
 * A DNS record served by the tunnel (injected via RFC 2136 or added manually).
 * `value` is the rdata as text: an IP for A/AAAA, a name for CNAME, etc.
 */
export type DnsRecordEntry = z.output<typeof schemas.DnsRecordEntry>
export type Dump = z.output<typeof schemas.Dump>
export type DumpParamsInput = z.input<typeof schemas.DumpParams>
export type GenerateCertParamsInput = z.input<typeof schemas.GenerateCertParams>
export type Guid = z.output<typeof schemas.Guid>

/**
 * Proof that a set of auth keys was unenrolled — removed from the persisted
 * and ephemeral signer stores with any continuations they opened killed.
 * Obtained via [`SignatureAuthContext::unenroll`], or [`Self::unenroll`]
 * from inside a db transaction.
 */
export type HasUnenrolledKeys = z.output<typeof schemas.HasUnenrolledKeys>
export type HttpRedirectStatus = z.output<typeof schemas.HttpRedirectStatus>
export type IoFormatInput = z.input<typeof schemas.IoFormat>
export type ListDevicesParamsInput = z.input<typeof schemas.ListDevicesParams>
export type ListParamsInput = z.input<typeof schemas.ListParams>
export type LogoutParamsInput = z.input<typeof schemas.LogoutParams>
export type RemoveDeviceParamsInput = z.input<typeof schemas.RemoveDeviceParams>
export type RemoveDnsRecordParamsInput = z.input<
  typeof schemas.RemoveDnsRecordParams
>
export type RemovePinholeParamsInput = z.input<
  typeof schemas.RemovePinholeParams
>
export type RemovePortForwardParamsInput = z.input<
  typeof schemas.RemovePortForwardParams
>
export type Session = z.output<typeof schemas.Session>
export type SetAutoPortForwardParamsInput = z.input<
  typeof schemas.SetAutoPortForwardParams
>
export type SetDeviceWanParamsInput = z.input<typeof schemas.SetDeviceWanParams>
export type SetDnsInjectionParamsInput = z.input<
  typeof schemas.SetDnsInjectionParams
>
export type SetHttpRedirectEnabledParamsInput = z.input<
  typeof schemas.SetHttpRedirectEnabledParams
>
export type SetListenParamsInput = z.input<typeof schemas.SetListenParams>
export type SetPasswordParamsInput = z.input<typeof schemas.SetPasswordParams>
export type SetPinholeEnabledParamsInput = z.input<
  typeof schemas.SetPinholeEnabledParams
>
export type SetPortForwardEnabledParamsInput = z.input<
  typeof schemas.SetPortForwardEnabledParams
>
export type SetSubnetIpv6ParamsInput = z.input<
  typeof schemas.SetSubnetIpv6Params
>
export type SetSubnetWanParamsInput = z.input<typeof schemas.SetSubnetWanParams>
export type ShowConfigParamsInput = z.input<typeof schemas.ShowConfigParams>
export type SubnetParamsInput = z.input<typeof schemas.SubnetParams>
export type SubscribeParamsInput = z.input<typeof schemas.SubscribeParams>
export type TunnelCertDataInput = z.input<typeof schemas.TunnelCertData>
export type TunnelUpdateResult = z.output<typeof schemas.TunnelUpdateResult>
export type UpdatePinholeLabelParamsInput = z.input<
  typeof schemas.UpdatePinholeLabelParams
>
export type UpdatePortForwardLabelParamsInput = z.input<
  typeof schemas.UpdatePortForwardLabelParams
>

/**
 * A WireGuard client's role. A `Server` is a StartOS box that may configure the
 * gateway (DNS injection / auto port-forward); a `Client` is a plain peer with
 * no autoconfig. Stored and sticky — toggling the capability flags never changes
 * it; the migration backfills it from those flags.
 */
export type WgClientKind = z.output<typeof schemas.WgClientKind>

/**
 * A WireGuard client's role. A `Server` is a StartOS box that may configure the
 * gateway (DNS injection / auto port-forward); a `Client` is a plain peer with
 * no autoconfig. Stored and sticky — toggling the capability flags never changes
 * it; the migration backfills it from those flags.
 */
export type WgClientKindInput = z.input<typeof schemas.WgClientKind>
export type AddDeviceParamsInput = z.input<typeof schemas.AddDeviceParams>
export type AddKeyParamsInput = z.input<typeof schemas.AddKeyParams>
export type ApplyWithPathParamsInput = z.input<
  typeof schemas.ApplyWithPathParams
>

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export type AuthKeys = z.output<typeof schemas.AuthKeys>
export type KillParamsInput = z.input<typeof schemas.KillParams>
export type LoginParamsInput = z.input<typeof schemas.LoginParams>
export type RemoveKeyParamsInput = z.input<typeof schemas.RemoveKeyParams>
export type SetDeviceKindParamsInput = z.input<
  typeof schemas.SetDeviceKindParams
>
export type SetSubnetDnsParamsInput = z.input<typeof schemas.SetSubnetDnsParams>
export type SubscribeRes = z.output<typeof schemas.SubscribeRes>
export type WgConfig = z.output<typeof schemas.WgConfig>
export type SessionList = z.output<typeof schemas.SessionList>
export type WgSubnetClients = z.output<typeof schemas.WgSubnetClients>
export type WgSubnetConfig = z.output<typeof schemas.WgSubnetConfig>
export type Api = RpcSchemaTree<typeof schemas.Api>
