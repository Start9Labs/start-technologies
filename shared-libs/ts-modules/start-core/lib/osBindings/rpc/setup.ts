import type { z } from 'zod'
import type * as schemas from './setup.schemas'
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
export type CapabilityVerdict = z.output<typeof schemas.CapabilityVerdict>
export type CifsInput = z.input<typeof schemas.Cifs>
export type DataDriveInput = z.input<typeof schemas.DataDrive>
export type DnsSource = z.output<typeof schemas.DnsSource>
export type EncryptedWireInput = z.input<typeof schemas.EncryptedWire>

/**
 * Unauthenticated discovery hints; opening the store validates its actual state.
 */
export type FormatStatus = z.output<typeof schemas.FormatStatus>
export type GatewayId = z.output<typeof schemas.GatewayId>
export type GatewayType = z.output<typeof schemas.GatewayType>
export type Guid = z.output<typeof schemas.Guid>
export type HostIdInput = z.input<typeof schemas.HostId>
export type IoFormatInput = z.input<typeof schemas.IoFormat>
export type KeyboardOptionsInput = z.input<typeof schemas.KeyboardOptions>
export type LogEntry = z.output<typeof schemas.LogEntry>
export type NamedProgress = z.output<typeof schemas.NamedProgress>
export type NetworkInterfaceType = z.output<typeof schemas.NetworkInterfaceType>
export type PackageIdInput = z.input<typeof schemas.PackageId>
export type PartitionTable = z.output<typeof schemas.PartitionTable>
export type ProgressUnits = z.output<typeof schemas.ProgressUnits>
export type QueryDnsParamsInput = z.input<typeof schemas.QueryDnsParams>

/**
 * What a public domain currently resolves to, per address family. A public
 * domain is DualStack: the operator points an `A` record at the gateway's WAN
 * IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
 */
export type QueryDnsRes = z.output<typeof schemas.QueryDnsRes>

/**
 * A migration or backup recovery source with a caller-selected backup password representation.
 */
export type RecoverySourceInput<Password> = z.input<
  ReturnType<typeof schemas.RecoverySource<z.ZodType<Password, Password>>>
>
export type ServerHostname = z.output<typeof schemas.ServerHostname>
export type SetLanguageParamsInput = z.input<typeof schemas.SetLanguageParams>
export type SetupInfo = z.output<typeof schemas.SetupInfo>
export type AttachParamsInput = z.input<typeof schemas.AttachParams>
export type BackupTargetFSInput = z.input<typeof schemas.BackupTargetFS>

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
export type InstallOsParamsInput = z.input<typeof schemas.InstallOsParams>
export type IpInfo = z.output<typeof schemas.IpInfo>
export type LogFollowResponse = z.output<typeof schemas.LogFollowResponse>
export type LogResponse = z.output<typeof schemas.LogResponse>
export type RequiresHostIdInput = z.input<typeof schemas.RequiresHostId>
export type RequiresPackageIdInput = z.input<typeof schemas.RequiresPackageId>
export type SetupExecuteParamsInput = z.input<typeof schemas.SetupExecuteParams>
export type SetupResult = z.output<typeof schemas.SetupResult>

/**
 * The public view of a backup found on a target: enough to identify it, and none of
 * [`BackupUnencryptedMetadata`]'s key material.
 */
export type StartOsRecoveryInfo = z.output<typeof schemas.StartOsRecoveryInfo>
export type VerifyCifsParamsInput = z.input<typeof schemas.VerifyCifsParams>
export type NetworkInterfaceInfo = z.output<typeof schemas.NetworkInterfaceInfo>
export type PartitionInfo = z.output<typeof schemas.PartitionInfo>
export type Progress = z.output<typeof schemas.Progress>
export type SetupProgress = z.output<typeof schemas.SetupProgress>
export type DiskInfo = z.output<typeof schemas.DiskInfo>
export type NetworkGatewayInfo = z.output<typeof schemas.NetworkGatewayInfo>
export type SetupStatusRes = z.output<typeof schemas.SetupStatusRes>

/**
 * Active DNS authority and its bootstrap gateway.
 */
export type DnsStatus = z.output<typeof schemas.DnsStatus>
export type Api = RpcSchemaTree<typeof schemas.Api>
