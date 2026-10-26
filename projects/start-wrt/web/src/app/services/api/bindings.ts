import type { z } from 'zod'
import type * as schemas from './bindings.schemas'
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
export type ActivityDeleteParamsInput = z.input<
  typeof schemas.ActivityDeleteParams
>
export type ActivityEntry = z.output<typeof schemas.ActivityEntry>
export type ActivityListParamsInput = z.input<typeof schemas.ActivityListParams>

/**
 * A published port that will be removed because the device it forwards to can
 * no longer be reached at its current address — its security profile is changing
 * (ethernet-port or WiFi-password reassignment) or its WiFi password is being
 * deleted (disconnecting it) — so its DNAT rule, bound to the device's old
 * subnet, would otherwise break. Surfaced to the UI to confirm the deletion.
 */
export type AffectedPublishedPort = z.output<
  typeof schemas.AffectedPublishedPort
>
export type AnyVerifyingKeyInput = z.input<typeof schemas.AnyVerifyingKey>
export type ApChannel = z.output<typeof schemas.ApChannel>

/**
 * One automatic port use for the published-ports UI.
 */
export type AutomaticPortUse = z.output<typeof schemas.AutomaticPortUse>
export type BlackoutSetResult = z.output<typeof schemas.BlackoutSetResult>
export type BlackoutWindow = z.output<typeof schemas.BlackoutWindow>
export type BlackoutWindowInput = z.input<typeof schemas.BlackoutWindow>
export type CheckInitializedRes = z.output<typeof schemas.CheckInitializedRes>
export type DataUsagePeriodInput = z.input<typeof schemas.DataUsagePeriod>
export type DataUsagePoint = z.output<typeof schemas.DataUsagePoint>
export type DdnsProvider = z.output<typeof schemas.DdnsProvider>
export type DdnsProviderInput = z.input<typeof schemas.DdnsProvider>
export type DeleteArgsInput = z.input<typeof schemas.DeleteArgs>
export type DeviceMacReqInput = z.input<typeof schemas.DeviceMacReq>
export type DeviceStatus = z.output<typeof schemas.DeviceStatus>
export type DeviceUpdateReqInput = z.input<typeof schemas.DeviceUpdateReq>
export type DirGetArgsInput = z.input<typeof schemas.DirGetArgs>
export type DiskState = z.output<typeof schemas.DiskState>
export type DnsMode = z.output<typeof schemas.DnsMode>
export type DnsModeInput = z.input<typeof schemas.DnsMode>

/**
 * A single DNS server entry with protocol info.
 * `ssl: false` = plain UDP on port 53.
 * `ssl: true`  = DNS-over-HTTPS via SmartDNS (`server-https`).
 */
export type DnsServer = z.output<typeof schemas.DnsServer>

/**
 * A single DNS server entry with protocol info.
 * `ssl: false` = plain UDP on port 53.
 * `ssl: true`  = DNS-over-HTTPS via SmartDNS (`server-https`).
 */
export type DnsServerInput = z.input<typeof schemas.DnsServer>
export type ExecReqInput = z.input<typeof schemas.ExecReq>
export type ExecRes = z.output<typeof schemas.ExecRes>
export type FileContents = z.output<typeof schemas.FileContents>
export type FileType = z.output<typeof schemas.FileType>
export type FollowLogsParamsInput = z.input<typeof schemas.FollowLogsParams>
export type GetArgsInput = z.input<typeof schemas.GetArgs>
export type GetFileArgsInput = z.input<typeof schemas.GetFileArgs>
export type Guid = z.output<typeof schemas.Guid>

/**
 * Proof that a set of auth keys was unenrolled — removed from the persisted
 * and ephemeral signer stores with any continuations they opened killed.
 * Obtained via [`SignatureAuthContext::unenroll`], or [`Self::unenroll`]
 * from inside a db transaction.
 */
export type HasUnenrolledKeys = z.output<typeof schemas.HasUnenrolledKeys>
export type InjectedDnsRecord = z.output<typeof schemas.InjectedDnsRecord>
export type IoFormatInput = z.input<typeof schemas.IoFormat>
export type KillParamsInput = z.input<typeof schemas.KillParams>
export type LanIpv4Response = z.output<typeof schemas.LanIpv4Response>
export type LanIpv4SetRequestInput = z.input<typeof schemas.LanIpv4SetRequest>
export type LanIpv6Response = z.output<typeof schemas.LanIpv6Response>
export type LanIpv6SetRequestInput = z.input<typeof schemas.LanIpv6SetRequest>
export type ListParamsInput = z.input<typeof schemas.ListParams>
export type LogEntry = z.output<typeof schemas.LogEntry>
export type LogoutParamsInput = z.input<typeof schemas.LogoutParams>
export type MacStrategy = z.output<typeof schemas.MacStrategy>
export type MacStrategyInput = z.input<typeof schemas.MacStrategy>
export type OutboundVpnCreateRequestInput = z.input<
  typeof schemas.OutboundVpnCreateRequest
>
export type OutboundVpnCreateResponse = z.output<
  typeof schemas.OutboundVpnCreateResponse
>
export type OutboundVpnDeleteRequestInput = z.input<
  typeof schemas.OutboundVpnDeleteRequest
>
export type OutboundVpnSetEnabledRequestInput = z.input<
  typeof schemas.OutboundVpnSetEnabledRequest
>
export type OutboundVpnStatus = z.output<typeof schemas.OutboundVpnStatus>
export type OutboundVpnUpdateRequestInput = z.input<
  typeof schemas.OutboundVpnUpdateRequest
>

/**
 * Response from adding a peer
 */
export type PeerAddResponse = z.output<typeof schemas.PeerAddResponse>
export type PeerDeleteArgsInput = z.input<typeof schemas.PeerDeleteArgs>
export type PeerRenameArgsInput = z.input<typeof schemas.PeerRenameArgs>
export type PeerSetDnsInjectionArgsInput = z.input<
  typeof schemas.PeerSetDnsInjectionArgs
>
export type ProfileId = z.output<typeof schemas.ProfileId>
export type ProfileIdOptInput = z.input<typeof schemas.ProfileIdOpt>
export type Protocol = z.output<typeof schemas.Protocol>
export type ProtocolInput = z.input<typeof schemas.Protocol>
export type PublishedPortStatus = z.output<typeof schemas.PublishedPortStatus>
export type ResetPasswordParamsInput = z.input<
  typeof schemas.ResetPasswordParams
>
export type ScheduleGetParamsInput = z.input<typeof schemas.ScheduleGetParams>
export type ScheduleWindow = z.output<typeof schemas.ScheduleWindow>
export type ScheduleWindowInput = z.input<typeof schemas.ScheduleWindow>
export type Section = z.output<typeof schemas.Section>
export type SectionInput = z.input<typeof schemas.Section>
export type Session = z.output<typeof schemas.Session>
export type SetAutoForwardRequestInput = z.input<
  typeof schemas.SetAutoForwardRequest
>
export type SetDnsInjectionReqInput = z.input<typeof schemas.SetDnsInjectionReq>
export type SetFileArgsInput = z.input<typeof schemas.SetFileArgs>
export type SetInitialPasswordParamsInput = z.input<
  typeof schemas.SetInitialPasswordParams
>
export type SetPreferencesReqInput = z.input<typeof schemas.SetPreferencesReq>
export type SetRouterAdvertisementsReqInput = z.input<
  typeof schemas.SetRouterAdvertisementsReq
>
export type SetTimezoneParamsInput = z.input<typeof schemas.SetTimezoneParams>
export type SniPortUse = z.output<typeof schemas.SniPortUse>
export type SpeedData = z.output<typeof schemas.SpeedData>
export type SshKeyAddParamsInput = z.input<typeof schemas.SshKeyAddParams>
export type SshKeyDeleteParamsInput = z.input<typeof schemas.SshKeyDeleteParams>
export type SshKeyResponse = z.output<typeof schemas.SshKeyResponse>
export type SystemInfoResponse = z.output<typeof schemas.SystemInfoResponse>
export type UpdateSystemParamsInput = z.input<typeof schemas.UpdateSystemParams>
export type UpdateSystemRes = z.output<typeof schemas.UpdateSystemRes>
export type VerifyPasswordParamsInput = z.input<
  typeof schemas.VerifyPasswordParams
>
export type VersionInfo = z.output<typeof schemas.VersionInfo>

/**
 * VPN server configuration for set/create requests
 */
export type VpnServerConfigInput = z.input<typeof schemas.VpnServerConfig>

/**
 * Peer configuration for VPN server
 */
export type VpnServerPeer = z.output<typeof schemas.VpnServerPeer>

/**
 * Peer configuration for VPN server
 */
export type VpnServerPeerInput = z.input<typeof schemas.VpnServerPeer>
export type WanAccess = z.output<typeof schemas.WanAccess>
export type WanAccessInput = z.input<typeof schemas.WanAccess>
export type WanIpv4Mode = z.output<typeof schemas.WanIpv4Mode>
export type WanIpv4ModeInput = z.input<typeof schemas.WanIpv4Mode>
export type WanIpv6Mode = z.output<typeof schemas.WanIpv6Mode>
export type WanIpv6ModeInput = z.input<typeof schemas.WanIpv6Mode>
export type WifiRadio = z.output<typeof schemas.WifiRadio>
export type WifiRadioInput = z.input<typeof schemas.WifiRadio>
export type ActivityListResponse = z.output<typeof schemas.ActivityListResponse>

/**
 * The server's enrolled auth keys, keyed by their PEM encoding. Each enrolled
 * key is a sign-in: it carries the same metadata a session used to (when it
 * was created, when it was last used, and the user agent that enrolled it).
 */
export type AuthKeys = z.output<typeof schemas.AuthKeys>
export type BackupCreateRes = z.output<typeof schemas.BackupCreateRes>
export type BackupRestoreRes = z.output<typeof schemas.BackupRestoreRes>
export type BandChannels = z.output<typeof schemas.BandChannels>
export type BlackoutWindowsInput = z.input<typeof schemas.BlackoutWindows>
export type DataUsageReqInput = z.input<typeof schemas.DataUsageReq>
export type Device = z.output<typeof schemas.Device>
export type DiagnosticsCreateRes = z.output<typeof schemas.DiagnosticsCreateRes>
export type DirEntry = z.output<typeof schemas.DirEntry>
export type EditArgsInput = z.input<typeof schemas.EditArgs>

/**
 * `ethernet.set` request: the desired port layout plus a confirmation flag.
 * Reassigning a port to a different profile moves its devices to a new subnet,
 * which breaks any published ports forwarding to them. When the flag is false
 * and such ports exist, `set` applies nothing and returns them in
 * `EthernetSetResult` for a confirmation dialog; with the flag true it deletes
 * those published ports as part of the reassignment.
 */
export type EthernetSetRequestInput = z.input<typeof schemas.EthernetSetRequest>
export type EthernetSetResult = z.output<typeof schemas.EthernetSetResult>
export type FollowLogsRes = z.output<typeof schemas.FollowLogsRes>
export type LoginParamsInput = z.input<typeof schemas.LoginParams>
export type LogsResponse = z.output<typeof schemas.LogsResponse>
export type OutboundVpn = z.output<typeof schemas.OutboundVpn>
export type PeerAddArgsInput = z.input<typeof schemas.PeerAddArgs>
export type ProfileSetRequestInput = z.input<typeof schemas.ProfileSetRequest>
export type PublishedPort = z.output<typeof schemas.PublishedPort>
export type PublishedPortInputInput = z.input<typeof schemas.PublishedPortInput>
export type ScheduleWindowsInput = z.input<typeof schemas.ScheduleWindows>
export type SetArgsInput = z.input<typeof schemas.SetArgs>
export type SetupStatusRes = z.output<typeof schemas.SetupStatusRes>
export type UciFile = z.output<typeof schemas.UciFile>
export type UciFileInput = z.input<typeof schemas.UciFile>

/**
 * VPN server configuration returned by list (excludes sensitive data)
 */
export type VpnServer = z.output<typeof schemas.VpnServer>
export type WanDdnsResponse = z.output<typeof schemas.WanDdnsResponse>
export type WanDdnsSetRequestInput = z.input<typeof schemas.WanDdnsSetRequest>
export type WanDnsResponse = z.output<typeof schemas.WanDnsResponse>
export type WanDnsSetRequestInput = z.input<typeof schemas.WanDnsSetRequest>
export type WanIpv4Response = z.output<typeof schemas.WanIpv4Response>
export type WanIpv4SetRequestInput = z.input<typeof schemas.WanIpv4SetRequest>
export type WanIpv6Response = z.output<typeof schemas.WanIpv6Response>
export type WanIpv6SetRequestInput = z.input<typeof schemas.WanIpv6SetRequest>
export type WanMacResponse = z.output<typeof schemas.WanMacResponse>
export type WanMacSetRequestInput = z.input<typeof schemas.WanMacSetRequest>

/**
 * An enabled IPv4 forward overlapping a router- or SNI-owned WAN port.
 */
export type WanPortCollision = z.output<typeof schemas.WanPortCollision>

/**
 * `wifi.set` request: the desired WiFi config plus a confirmation flag.
 * When a profile loses its last WiFi password its devices can no longer reach
 * that subnet — disconnected if the password was removed, or moved to another
 * profile if it was reassigned — which breaks any published ports forwarding to
 * them. When the flag is false and such ports exist, `set` applies nothing and
 * returns them in `WifiSetResult` for a confirmation dialog; with the flag true
 * it deletes those published ports as part of the change.
 */
export type WifiSetRequestInput = z.input<typeof schemas.WifiSetRequest>
export type WifiSetResult = z.output<typeof schemas.WifiSetResult>

/**
 * `wifi.blackout-set` request: the blackout windows plus a confirmation flag.
 * A window covering the router's current time turns Wi-Fi off as soon as it is
 * saved, for every device on it. When the flag is false, `blackout-set` applies
 * nothing and reports that in `BlackoutSetResult` for a confirmation dialog;
 * with the flag true it saves the windows and applies them at once.
 */
export type BlackoutSetRequestInput = z.input<typeof schemas.BlackoutSetRequest>
export type PublishedPortsSetRequestInput = z.input<
  typeof schemas.PublishedPortsSetRequest
>

/**
 * [`set`] response. A non-empty collision list means nothing was applied —
 * the caller confirms and re-saves; empty means the request was applied.
 */
export type PublishedPortsSetResult = z.output<
  typeof schemas.PublishedPortsSetResult
>
export type SessionList = z.output<typeof schemas.SessionList>

/**
 * Response containing all VPN servers
 */
export type VpnServers = z.output<typeof schemas.VpnServers>
export type WifiRegulatory = z.output<typeof schemas.WifiRegulatory>
export type Api = RpcSchemaTree<typeof schemas.Api>
