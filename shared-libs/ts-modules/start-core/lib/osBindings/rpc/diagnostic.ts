import type { z } from 'zod'
import type * as schemas from './diagnostic.schemas'
export type RpcHandler = {
  _PARAMS: unknown
  _RETURN?: unknown
  _ITEM?: unknown
  _FINAL?: unknown
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

export type RpcEndpointType<
  Root extends RpcHandler,
  Method extends string,
> = Method extends ''
  ? Root extends { _RETURN: unknown }
    ? Root
    : never
  : Root extends { _CHILDREN: infer Children }
    ? Method extends `${infer Head}.${infer Tail}`
      ? Head extends keyof Children
        ? Children[Head] extends RpcHandler
          ? RpcEndpointType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? RpcEndpointType<Children[Method], ''>
          : never
        : never
    : never

export type RpcEndpointField<
  Endpoint,
  Field extends PropertyKey,
> = Endpoint extends { _RETURN: unknown }
  ? Field extends keyof Endpoint
    ? Endpoint[Field]
    : unknown
  : never

export type RpcParamType<
  Root extends RpcHandler,
  Method extends string,
> = RpcEndpointField<RpcEndpointType<Root, Method>, '_PARAMS'>

export type RpcReturnType<
  Root extends RpcHandler,
  Method extends string,
> = RpcEndpointField<RpcEndpointType<Root, Method>, '_RETURN'>

export type RpcStreamItemType<
  Root extends RpcHandler,
  Method extends string,
> = RpcEndpointField<RpcEndpointType<Root, Method>, '_ITEM'>

export type RpcStreamFinalType<
  Root extends RpcHandler,
  Method extends string,
> = RpcEndpointField<RpcEndpointType<Root, Method>, '_FINAL'>

export type RpcIsStreaming<Root extends RpcHandler, Method extends string> = [
  RpcEndpointType<Root, Method>,
] extends [never]
  ? never
  : unknown extends RpcStreamItemType<Root, Method>
    ? boolean
    : [RpcStreamItemType<Root, Method>] extends [never]
      ? false
      : true

export type RpcStreamingMethod<Root extends RpcHandler> = {
  [Method in RpcMethod<Root>]: RpcIsStreaming<Root, Method> extends false
    ? never
    : Method
}[RpcMethod<Root>]

export type RpcSchemaTree<Schema> = {
  [Key in keyof Schema as Key extends '_OPAQUE'
    ? never
    : Key]: Key extends '_PARAMS'
    ? Schema[Key] extends z.ZodType
      ? z.input<Schema[Key]>
      : never
    : Key extends '_RETURN' | '_ITEM' | '_FINAL'
      ? Schema[Key] extends z.ZodType
        ? z.output<Schema[Key]>
        : never
      : Key extends '_CHILDREN'
        ? { [Name in keyof Schema[Key]]: RpcSchemaTree<Schema[Key][Name]> }
        : never
}
export type ApiState = z.output<typeof schemas.ApiState>
export type CapabilityVerdict = z.output<typeof schemas.CapabilityVerdict>
export type DiagnosticUpdateSystemParamsInput = z.input<
  typeof schemas.DiagnosticUpdateSystemParams
>
export type DnsSource = z.output<typeof schemas.DnsSource>
export type GatewayId = z.output<typeof schemas.GatewayId>
export type GatewayType = z.output<typeof schemas.GatewayType>
export type Guid = z.output<typeof schemas.Guid>
export type HostIdInput = z.input<typeof schemas.HostId>
export type IoFormatInput = z.input<typeof schemas.IoFormat>
export type LogEntry = z.output<typeof schemas.LogEntry>
export type NamedProgress = z.output<typeof schemas.NamedProgress>
export type NetworkInterfaceType = z.output<typeof schemas.NetworkInterfaceType>
export type PackageIdInput = z.input<typeof schemas.PackageId>
export type ProgressUnits = z.output<typeof schemas.ProgressUnits>
export type QueryDnsParamsInput = z.input<typeof schemas.QueryDnsParams>

/**
 * What a public domain currently resolves to, per address family. A public
 * domain is DualStack: the operator points an `A` record at the gateway's WAN
 * IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
 */
export type QueryDnsRes = z.output<typeof schemas.QueryDnsRes>

/**
 * Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted
 * override; `servers` and `source` are derived from it and the link configuration.
 * Without an override or gateway-provided resolvers, the public fallback is used.
 */
export type DnsSettings = z.output<typeof schemas.DnsSettings>
export type FullProgress = z.output<typeof schemas.FullProgress>

/**
 * Whether the gateway reachable via this interface speaks each port-mapping
 * protocol, as last probed. Fed by the watcher's periodic probes and by
 * failure/success evidence from the port-map client, and synced to the db so a
 * chronically uncooperative gateway is visible (and skipped) instead of being
 * retried forever.
 */
export type GatewayPortMapCapabilities = z.output<
  typeof schemas.GatewayPortMapCapabilities
>
export type IpInfo = z.output<typeof schemas.IpInfo>
export type LogFollowResponse = z.output<typeof schemas.LogFollowResponse>
export type LogResponse = z.output<typeof schemas.LogResponse>
export type RequiresHostIdInput = z.input<typeof schemas.RequiresHostId>
export type RequiresPackageIdInput = z.input<typeof schemas.RequiresPackageId>
export type UpdateSystemRes = z.output<typeof schemas.UpdateSystemRes>
export type NetworkInterfaceInfo = z.output<typeof schemas.NetworkInterfaceInfo>
export type Progress = z.output<typeof schemas.Progress>
export type NetworkGatewayInfo = z.output<typeof schemas.NetworkGatewayInfo>

/**
 * Active DNS authority and its bootstrap gateway.
 */
export type DnsStatus = z.output<typeof schemas.DnsStatus>
export type Api = RpcSchemaTree<typeof schemas.Api>
