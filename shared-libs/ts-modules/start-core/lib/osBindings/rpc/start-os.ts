import type { z } from 'zod'
import type * as schemas from './start-os.schemas'
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
export type AcmeProvider = z.output<typeof schemas.AcmeProvider>
export type AcmeProviderInput = z.input<typeof schemas.AcmeProvider>
export type ActionId = z.output<typeof schemas.ActionId>
export type ActionIdInput = z.input<typeof schemas.ActionId>
export type ActionResultV0 = z.output<typeof schemas.ActionResultV0>

/**
 * JSON Patch 'add' operation representation
 */
export type AddOperation = z.output<typeof schemas.AddOperation>

/**
 * Keys an import added, and keys it found already present.
 */
export type AddTrustedKeyRes = z.output<typeof schemas.AddTrustedKeyRes>
export type AnySignatureInput = z.input<typeof schemas.AnySignature>
export type AnyVerifyingKey = z.output<typeof schemas.AnyVerifyingKey>
export type AnyVerifyingKeyInput = z.input<typeof schemas.AnyVerifyingKey>
export type ApiState = z.output<typeof schemas.ApiState>
export type ApplyParamsInput = z.input<typeof schemas.ApplyParams>

/**
 * A registered-target identity; migrated CIFS targets retain their existing IDs.
 */
export type BackupTargetId = z.output<typeof schemas.BackupTargetId>

/**
 * A registered-target identity; migrated CIFS targets retain their existing IDs.
 */
export type BackupTargetIdInput = z.input<typeof schemas.BackupTargetId>
export type BasicCredential = z.output<typeof schemas.BasicCredential>
export type CapabilityVerdict = z.output<typeof schemas.CapabilityVerdict>
export type Celsius = z.output<typeof schemas.Celsius>
export type CheckPortRes = z.output<typeof schemas.CheckPortRes>

/**
 * v6 reachability of the box's GUA at a port. v6 is NAT-free (the GUA is the
 * box's own address), so there is no hairpinning. Queried separately from
 * [`CheckPortRes`] so the IPv4 and IPv6 checks can run independently.
 */
export type CheckPortV6Res = z.output<typeof schemas.CheckPortV6Res>
export type CifsInput = z.input<typeof schemas.Cifs>
export type CifsAddParamsInput = z.input<typeof schemas.CifsAddParams>
export type ConnectRegistryParamsInput = z.input<
  typeof schemas.ConnectRegistryParams
>
export type ContactInfo = z.output<typeof schemas.ContactInfo>
export type ContainerId = z.output<typeof schemas.ContainerId>

/**
 * JSON Patch 'copy' operation representation
 */
export type CopyOperation = z.output<typeof schemas.CopyOperation>
export type CreateTokenParamsInput = z.input<typeof schemas.CreateTokenParams>
export type CreatedToken = z.output<typeof schemas.CreatedToken>
export type DataUrl = z.output<typeof schemas.DataUrl>
export type DbCursorInput = z.input<typeof schemas.DbCursor>
export type DnsSource = z.output<typeof schemas.DnsSource>
export type Dump = z.output<typeof schemas.Dump>
export type DumpParamsInput = z.input<typeof schemas.DumpParams>
export type EchoParamsInput = z.input<typeof schemas.EchoParams>
export type EncryptedWireInput = z.input<typeof schemas.EncryptedWire>
export type EndpointParamsInput = z.input<typeof schemas.EndpointParams>

/**
 * Selects how aggressively an EPP-capable CPU pursues performance.
 */
export type Epp = z.output<typeof schemas.Epp>

/**
 * Selects how aggressively an EPP-capable CPU pursues performance.
 */
export type EppInput = z.input<typeof schemas.Epp>
export type ExecResult = z.output<typeof schemas.ExecResult>
export type FormatDiskParamsInput = z.input<typeof schemas.FormatDiskParams>

/**
 * Unauthenticated discovery hints; opening the store validates its actual state.
 */
export type FormatStatus = z.output<typeof schemas.FormatStatus>
export type ForwardTarget = z.output<typeof schemas.ForwardTarget>
export type GatewayId = z.output<typeof schemas.GatewayId>
export type GatewayIdInput = z.input<typeof schemas.GatewayId>
export type GatewayType = z.output<typeof schemas.GatewayType>
export type GatewayTypeInput = z.input<typeof schemas.GatewayType>
export type GenerateCertificateParamsInput = z.input<
  typeof schemas.GenerateCertificateParams
>
export type GenerateCertificateResponse = z.output<
  typeof schemas.GenerateCertificateResponse
>
export type GigaBytes = z.output<typeof schemas.GigaBytes>
export type Governor = z.output<typeof schemas.Governor>
export type GovernorInput = z.input<typeof schemas.Governor>

/**
 * An original public-key export and its self-certified identity claims.
 */
export type GpgPublicKeyExport = z.output<typeof schemas.GpgPublicKeyExport>
export type Guid = z.output<typeof schemas.Guid>
export type GuidInput = z.input<typeof schemas.Guid>

/**
 * Proof that a set of auth keys was unenrolled — removed from the persisted
 * and ephemeral signer stores with any continuations they opened killed.
 * Obtained via [`SignatureAuthorizationContext::unenroll`], or [`Self::unenroll`]
 * from inside a db transaction.
 */
export type HasUnenrolledKeys = z.output<typeof schemas.HasUnenrolledKeys>
export type HistoryParamsInput = z.input<typeof schemas.HistoryParams>
export type HostId = z.output<typeof schemas.HostId>
export type HostIdInput = z.input<typeof schemas.HostId>
export type ImageIdInput = z.input<typeof schemas.ImageId>
export type IoFormatInput = z.input<typeof schemas.IoFormat>
export type KeyboardOptionsInput = z.input<typeof schemas.KeyboardOptions>
export type ListNotificationParamsInput = z.input<
  typeof schemas.ListNotificationParams
>

/**
 * Address of the NUT server to list UPS devices from.
 */
export type ListNutUpsParamsInput = z.input<typeof schemas.ListNutUpsParams>
export type ListParamsInput = z.input<typeof schemas.ListParams>
export type LogEntry = z.output<typeof schemas.LogEntry>

/**
 * Verbosity accepted by the server's runtime logging control.
 */
export type LogLevelInput = z.input<typeof schemas.LogLevel>
export type LogoutParamsInput = z.input<typeof schemas.LogoutParams>
export type LshwDisplay = z.output<typeof schemas.LshwDisplay>
export type LshwProcessor = z.output<typeof schemas.LshwProcessor>
export type MaybeUtf8String = z.output<typeof schemas.MaybeUtf8String>
export type MebiBytes = z.output<typeof schemas.MebiBytes>
export type MetricsFollowParamsInput = z.input<
  typeof schemas.MetricsFollowParams
>
export type MiB = z.output<typeof schemas.MiB>
export type ModifyNotificationBeforeParamsInput = z.input<
  typeof schemas.ModifyNotificationBeforeParams
>
export type ModifyNotificationParamsInput = z.input<
  typeof schemas.ModifyNotificationParams
>

/**
 * JSON Patch 'move' operation representation
 */
export type MoveOperation = z.output<typeof schemas.MoveOperation>
export type NamedProgress = z.output<typeof schemas.NamedProgress>
export type NetInfo = z.output<typeof schemas.NetInfo>
export type NetworkInterfaceType = z.output<typeof schemas.NetworkInterfaceType>
export type NotificationLevel = z.output<typeof schemas.NotificationLevel>
export type NotificationLevelInput = z.input<typeof schemas.NotificationLevel>

/**
 * A UPS detected on USB, with the driver settings that address it.
 */
export type NutDevice = z.output<typeof schemas.NutDevice>

/**
 * A UPS detected on USB, with the driver settings that address it.
 */
export type NutDeviceInput = z.input<typeof schemas.NutDevice>

/**
 * A UPS served by a NUT server, as `LIST UPS` reports it.
 */
export type NutRemoteUps = z.output<typeof schemas.NutRemoteUps>

/**
 * Current variables reported by the configured UPS.
 */
export type NutStatus = z.output<typeof schemas.NutStatus>

/**
 * An absent update preserves DNS intent; null servers select gateway DNS.
 */
export type OutboundDnsUpdateInput = z.input<typeof schemas.OutboundDnsUpdate>
export type PackageBackupReport = z.output<typeof schemas.PackageBackupReport>
export type PackageId = z.output<typeof schemas.PackageId>
export type PackageIdInput = z.input<typeof schemas.PackageId>

/**
 * A persisted WebAuthn credential independent of browser session expiry.
 */
export type Passkey = z.output<typeof schemas.Passkey>
export type Percentage = z.output<typeof schemas.Percentage>
export type ProgressUnits = z.output<typeof schemas.ProgressUnits>
export type PushKeys = z.output<typeof schemas.PushKeys>
export type PushKeysInput = z.input<typeof schemas.PushKeys>
export type QueryDnsParamsInput = z.input<typeof schemas.QueryDnsParams>

/**
 * What a public domain currently resolves to, per address family. A public
 * domain is DualStack: the operator points an `A` record at the gateway's WAN
 * IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
 */
export type QueryDnsRes = z.output<typeof schemas.QueryDnsRes>
export type ReassignPortParamsInput = z.input<typeof schemas.ReassignPortParams>
export type RemoveDomainParamsInput = z.input<typeof schemas.RemoveDomainParams>

/**
 * JSON Patch 'remove' operation representation
 */
export type RemoveOperation = z.output<typeof schemas.RemoveOperation>
export type RemovePassthroughParamsInput = z.input<
  typeof schemas.RemovePassthroughParams
>
export type RemoveTrustedKeyParamsInput = z.input<
  typeof schemas.RemoveTrustedKeyParams
>
export type RemoveTrustedKeysParamsInput = z.input<
  typeof schemas.RemoveTrustedKeysParams
>

/**
 * JSON Patch 'replace' operation representation
 */
export type ReplaceOperation = z.output<typeof schemas.ReplaceOperation>
export type ReplayIdInput = z.input<typeof schemas.ReplayId>
export type RequiresReboot = z.output<typeof schemas.RequiresReboot>
export type Security = z.output<typeof schemas.Security>
export type ServerBackupReport = z.output<typeof schemas.ServerBackupReport>
export type ServerHostname = z.output<typeof schemas.ServerHostname>
export type ServiceInterfaceId = z.output<typeof schemas.ServiceInterfaceId>
export type ServiceInterfaceType = z.output<typeof schemas.ServiceInterfaceType>
export type Session = z.output<typeof schemas.Session>
export type SetCountryParamsInput = z.input<typeof schemas.SetCountryParams>
export type SetEchoipUrlsParamsInput = z.input<
  typeof schemas.SetEchoipUrlsParams
>
export type SetLanguageParamsInput = z.input<typeof schemas.SetLanguageParams>
export type SetServerHostnameParamsInput = z.input<
  typeof schemas.SetServerHostnameParams
>
export type SetWifiEnabledParamsInput = z.input<
  typeof schemas.SetWifiEnabledParams
>
export type SharedVolumeAvailability = z.output<
  typeof schemas.SharedVolumeAvailability
>
export type SharedVolumeBackupInfo = z.output<
  typeof schemas.SharedVolumeBackupInfo
>
export type SharedVolumeDrive = z.output<typeof schemas.SharedVolumeDrive>
export type SharedVolumeGrantInput = z.input<typeof schemas.SharedVolumeGrant>

/**
 * A 32-character uppercase base32 volume identity.
 */
export type SharedVolumeId = z.output<typeof schemas.SharedVolumeId>

/**
 * A 32-character uppercase base32 volume identity.
 */
export type SharedVolumeIdInput = z.input<typeof schemas.SharedVolumeId>
export type ShutdownParamsInput = z.input<typeof schemas.ShutdownParams>

/**
 * So a signal strength is a number between 0-100, I want the null option to be 0 since there is no signal
 */
export type SignalStrength = z.output<typeof schemas.SignalStrength>
export type SmtpSecurityInput = z.input<typeof schemas.SmtpSecurity>
export type SshDeleteParamsInput = z.input<typeof schemas.SshDeleteParams>
export type SshKeyResponse = z.output<typeof schemas.SshKeyResponse>
export type SshPubKeyInput = z.input<typeof schemas.SshPubKey>

/**
 * Ssid are the names of the wifis, usually human readable.
 */
export type Ssid = z.output<typeof schemas.Ssid>
export type TermSizeInput = z.input<typeof schemas.TermSize>

/**
 * JSON Patch 'test' operation representation
 */
export type TestOperation = z.output<typeof schemas.TestOperation>
export type TimeInfo = z.output<typeof schemas.TimeInfo>
export type TransportProtocol = z.output<typeof schemas.TransportProtocol>
export type TrustCaParamsInput = z.input<typeof schemas.TrustCaParams>
export type TrustContextInput = z.input<typeof schemas.TrustContext>
export type TrustedCa = z.output<typeof schemas.TrustedCa>
export type TrustedRegistryParamsInput = z.input<
  typeof schemas.TrustedRegistryParams
>
export type UiParamsInput = z.input<typeof schemas.UiParams>

/**
 * Changes a key's name and both trust selections atomically.
 */
export type UpdateTrustedKeyParamsInput = z.input<
  typeof schemas.UpdateTrustedKeyParams
>

/**
 * How the OS reverse proxy validates the container's TLS certificate when it
 * rewraps SSL (`add_ssl` set AND `secure.ssl == true`, so the OS terminates
 * the client's TLS and initiates a fresh TLS connection to the container).
 * Absent (`None`) means validate against the StartOS root CA — the default.
 */
export type UpstreamCertValidation = z.output<
  typeof schemas.UpstreamCertValidation
>
export type Version = z.output<typeof schemas.Version>
export type VersionInput = z.input<typeof schemas.Version>
export type WifiAddParamsInput = z.input<typeof schemas.WifiAddParams>
export type WifiSsidParamsInput = z.input<typeof schemas.WifiSsidParams>
export type ZramParamsInput = z.input<typeof schemas.ZramParams>
export type AcceptSignersInput = z.input<typeof schemas.AcceptSigners>
export type ActionInput = z.output<typeof schemas.ActionInput>
export type ActionResultValue = z.output<typeof schemas.ActionResultValue>
export type AddPasskeyParamsInput = z.input<typeof schemas.AddPasskeyParams>
export type AddPassthroughParamsInput = z.input<
  typeof schemas.AddPassthroughParams
>
export type AddPrivateDomainParamsInput = z.input<
  typeof schemas.AddPrivateDomainParams
>
export type AddPublicDomainParamsInput = z.input<
  typeof schemas.AddPublicDomainParams
>
export type AddTrustedKeyParamsInput = z.input<
  typeof schemas.AddTrustedKeyParams
>
export type AddTunnelParamsInput = z.input<typeof schemas.AddTunnelParams>
export type AddressInfo = z.output<typeof schemas.AddressInfo>

/**
 * The protocols a binding answers with, carried on the wire as the list itself.
 */
export type AlpnInfo = z.output<typeof schemas.AlpnInfo>
export type AttachParamsInput = z.input<typeof schemas.AttachParams>

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export type AuthKeys = z.output<typeof schemas.AuthKeys>
export type CancelInstallParamsInput = z.input<
  typeof schemas.CancelInstallParams
>
export type CheckChallengeParamsInput = z.input<
  typeof schemas.CheckChallengeParams
>

/**
 * Reachability of the port a certificate authority validates on. A null leg
 * went unprobed.
 */
export type CheckChallengeRes = z.output<typeof schemas.CheckChallengeRes>
export type CheckDnsParamsInput = z.input<typeof schemas.CheckDnsParams>
export type CheckPortParamsInput = z.input<typeof schemas.CheckPortParams>
export type CifsRemoveParamsInput = z.input<typeof schemas.CifsRemoveParams>
export type CifsUpdateParamsInput = z.input<typeof schemas.CifsUpdateParams>
export type ClearTaskParamsInput = z.input<typeof schemas.ClearTaskParams>
export type ControlParamsInput = z.input<typeof schemas.ControlParams>
export type CreateParamsInput = z.input<typeof schemas.CreateParams>
export type CreateSharedVolumeSourceInput = z.input<
  typeof schemas.CreateSharedVolumeSource
>
export type DeleteLegacyParamsInput = z.input<typeof schemas.DeleteLegacyParams>

/**
 * Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted
 * override; `servers` and `source` are derived from it and the link configuration.
 * Without an override or gateway-provided resolvers, the public fallback is used.
 */
export type DnsSettings = z.output<typeof schemas.DnsSettings>
export type EditGatewayParamsInput = z.input<typeof schemas.EditGatewayParams>
export type EnrollPasskeySessionParamsInput = z.input<
  typeof schemas.EnrollPasskeySessionParams
>
export type EppInfo = z.output<typeof schemas.EppInfo>
export type EppParamsInput = z.input<typeof schemas.EppParams>
export type ExecParamsInput = z.input<typeof schemas.ExecParams>
export type ForgetGatewayParamsInput = z.input<
  typeof schemas.ForgetGatewayParams
>
export type ForwardTable = z.output<typeof schemas.ForwardTable>
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
export type GetActionInputParamsInput = z.input<
  typeof schemas.GetActionInputParams
>
export type GovernorInfo = z.output<typeof schemas.GovernorInfo>
export type GovernorParamsInput = z.input<typeof schemas.GovernorParams>
export type HostnameMetadata = z.output<typeof schemas.HostnameMetadata>
export type HostnameMetadataInput = z.input<typeof schemas.HostnameMetadata>
export type InfoParamsInput = z.input<typeof schemas.InfoParams>
export type InitAcmeParamsInput = z.input<typeof schemas.InitAcmeParams>
export type InstalledVersionParamsInput = z.input<
  typeof schemas.InstalledVersionParams
>
export type IpInfo = z.output<typeof schemas.IpInfo>
export type KillParamsInput = z.input<typeof schemas.KillParams>
export type LegacyAlpnInfo = z.output<typeof schemas.LegacyAlpnInfo>
export type LogFollowResponse = z.output<typeof schemas.LogFollowResponse>
export type LogResponse = z.output<typeof schemas.LogResponse>
export type LoginParamsInput = z.input<typeof schemas.LoginParams>
export type LshwDevice = z.output<typeof schemas.LshwDevice>
export type MetricsCpu = z.output<typeof schemas.MetricsCpu>
export type MetricsDisk = z.output<typeof schemas.MetricsDisk>
export type MetricsGeneral = z.output<typeof schemas.MetricsGeneral>
export type MetricsMemory = z.output<typeof schemas.MetricsMemory>
export type MountParamsInput = z.input<typeof schemas.MountParams>
export type Notification = z.output<typeof schemas.Notification>

/**
 * Directly connected UPS server settings or remote NUT client settings.
 */
export type NutSettingsInput = z.input<typeof schemas.NutSettings>
export type OsInfo = z.output<typeof schemas.OsInfo>
export type PackageBackupInfo = z.output<typeof schemas.PackageBackupInfo>
export type PackageIdParamsInput = z.input<typeof schemas.PackageIdParams>

/**
 * What an unauthenticated browser needs to request and attribute a passkey signature.
 */
export type PasskeyCredential = z.output<typeof schemas.PasskeyCredential>
export type PassthroughInfo = z.output<typeof schemas.PassthroughInfo>
export type PasswordTypeInput = z.input<typeof schemas.PasswordType>

/**
 * JSON Patch single patch operation
 */
export type PatchOperation = z.output<typeof schemas.PatchOperation>

/**
 * Auth gate enforced by the OS reverse proxy on incoming requests.
 *
 * - `Bearer { tokens }`: any of `tokens` is accepted as `Authorization: Bearer <token>`.
 * - `Basic  { credentials }`: any `(username, password)` pair in `credentials` is
 *   accepted as `Authorization: Basic <base64(username:password)>`. The matched
 *   `username` is forwarded upstream as `X-Forwarded-User`.
 *
 * `realm` is the authentication realm advertised in the
 * `WWW-Authenticate` challenge sent on 401 responses (RFC 7235
 * §2.2). Defaults to `"StartOS"` when unset. Packages that share
 * credentials across multiple bindings should pick a stable realm
 * so that browsers reuse cached credentials across them.
 */
export type ProxyAuth = z.output<typeof schemas.ProxyAuth>
export type PublicDomainConfig = z.output<typeof schemas.PublicDomainConfig>
export type PushSubscription = z.output<typeof schemas.PushSubscription>
export type PushSubscriptionInput = z.input<typeof schemas.PushSubscription>
export type RebuildParamsInput = z.input<typeof schemas.RebuildParams>
export type RegistryIdentity = z.output<typeof schemas.RegistryIdentity>
export type RemoveAcmeParamsInput = z.input<typeof schemas.RemoveAcmeParams>
export type RemovePasskeyParamsInput = z.input<
  typeof schemas.RemovePasskeyParams
>
export type RemoveTunnelParamsInput = z.input<typeof schemas.RemoveTunnelParams>
export type RenameGatewayParamsInput = z.input<
  typeof schemas.RenameGatewayParams
>
export type RenameSharedVolumeParamsInput = z.input<
  typeof schemas.RenameSharedVolumeParams
>
export type RenameTargetParamsInput = z.input<typeof schemas.RenameTargetParams>
export type RequiresHostIdInput = z.input<typeof schemas.RequiresHostId>
export type RequiresPackageIdInput = z.input<typeof schemas.RequiresPackageId>
export type RestorePackageParamsInput = z.input<
  typeof schemas.RestorePackageParams
>
export type RestoreVolumeParamsInput = z.input<
  typeof schemas.RestoreVolumeParams
>
export type RestoreVolumeResult = z.output<typeof schemas.RestoreVolumeResult>
export type RunActionParamsInput = z.input<typeof schemas.RunActionParams>
export type ServiceStats = z.output<typeof schemas.ServiceStats>
export type SetDefaultOutboundParamsInput = z.input<
  typeof schemas.SetDefaultOutboundParams
>
export type SetGatewaySecureParamsInput = z.input<
  typeof schemas.SetGatewaySecureParams
>

/**
 * Sets verbosity for StartOS backend code in the running daemon.
 */
export type SetLogLevelParamsInput = z.input<typeof schemas.SetLogLevelParams>
export type SetOutboundGatewayParamsInput = z.input<
  typeof schemas.SetOutboundGatewayParams
>
export type SetSharedVolumeGrantParamsInput = z.input<
  typeof schemas.SetSharedVolumeGrantParams
>
export type SetStaticDnsParamsInput = z.input<typeof schemas.SetStaticDnsParams>
export type SharedVolumeInspection = z.output<
  typeof schemas.SharedVolumeInspection
>
export type SharedVolumeParamsInput = z.input<typeof schemas.SharedVolumeParams>
export type SharedVolumeSource = z.output<typeof schemas.SharedVolumeSource>
export type SideloadResponse = z.output<typeof schemas.SideloadResponse>
export type SmtpValueInput = z.input<typeof schemas.SmtpValue>
export type SshAddParamsInput = z.input<typeof schemas.SshAddParams>

/**
 * The public view of a backup found on a target: enough to identify it, and none of
 * [`BackupUnencryptedMetadata`]'s key material.
 */
export type StartOsRecoveryInfo = z.output<typeof schemas.StartOsRecoveryInfo>
export type SubscribeParamsInput = z.input<typeof schemas.SubscribeParams>
export type SubscribeRes = z.output<typeof schemas.SubscribeRes>
export type TargetParamsInput = z.input<typeof schemas.TargetParams>
export type TestSmtpParamsInput = z.input<typeof schemas.TestSmtpParams>
export type TransportWire = z.output<typeof schemas.TransportWire>
export type UmountParamsInput = z.input<typeof schemas.UmountParams>
export type UninstallParamsInput = z.input<typeof schemas.UninstallParams>
export type UnsetGatewaySecureParamsInput = z.input<
  typeof schemas.UnsetGatewaySecureParams
>
export type UpdateCifsSharedVolumeParamsInput = z.input<
  typeof schemas.UpdateCifsSharedVolumeParams
>
export type UpdateCifsTargetParamsInput = z.input<
  typeof schemas.UpdateCifsTargetParams
>
export type UpdateSystemRes = z.output<typeof schemas.UpdateSystemRes>
export type UpdateTunnelParamsInput = z.input<typeof schemas.UpdateTunnelParams>
export type VolumeBackupReport = z.output<typeof schemas.VolumeBackupReport>
export type WifiListOut = z.output<typeof schemas.WifiListOut>
export type ActionResultMember = z.output<typeof schemas.ActionResultMember>
export type ActionResultV1 = z.output<typeof schemas.ActionResultV1>
export type AddPublicDomainRes = z.output<typeof schemas.AddPublicDomainRes>
export type BackupInfo = z.output<typeof schemas.BackupInfo>
export type BackupParamsInput = z.input<typeof schemas.BackupParams>
export type BackupReport = z.output<typeof schemas.BackupReport>

/**
 * A registered backup destination and its last source observation.
 */
export type BackupTarget = z.output<typeof schemas.BackupTarget>
export type CompatibleAlpnInfo = z.output<typeof schemas.CompatibleAlpnInfo>

/**
 * Selects an existing server backup and its unlocking credential.
 */
export type ConvertParamsInput = z.input<typeof schemas.ConvertParams>
export type CreateSharedVolumeParamsInput = z.input<
  typeof schemas.CreateSharedVolumeParams
>
export type HardwareInfo = z.output<typeof schemas.HardwareInfo>
export type HostAddress = z.output<typeof schemas.HostAddress>
export type HostnameInfo = z.output<typeof schemas.HostnameInfo>
export type HostnameInfoInput = z.input<typeof schemas.HostnameInfo>
export type InstallParamsInput = z.input<typeof schemas.InstallParams>
export type Metrics = z.output<typeof schemas.Metrics>
export type NetworkInterfaceInfo = z.output<typeof schemas.NetworkInterfaceInfo>
export type NotificationWithId = z.output<typeof schemas.NotificationWithId>

/**
 * Stored NUT configuration and its independent enabled state.
 */
export type NutConfigInput = z.input<typeof schemas.NutConfig>

/**
 * Representation of JSON Patch (list of patch operations)
 */
export type Patch = z.output<typeof schemas.Patch>
export type Progress = z.output<typeof schemas.Progress>
export type ProxyOptions = z.output<typeof schemas.ProxyOptions>
export type ResetPasswordParamsInput = z.input<
  typeof schemas.ResetPasswordParams
>
export type ServiceInterface = z.output<typeof schemas.ServiceInterface>

/**
 * The server's sessions, by credential type.
 */
export type Sessions = z.output<typeof schemas.Sessions>
export type SideloadParamsInput = z.input<typeof schemas.SideloadParams>

/**
 * Non-empty selected transports, serialized as a sorted, deduplicated array.
 * A protocol string is accepted as a singleton. Omission selects TCP and UDP.
 */
export type Transport = z.output<typeof schemas.Transport>

/**
 * Signing-key material and independent package and OS-update trust.
 */
export type TrustedKey = z.output<typeof schemas.TrustedKey>
export type UpdateSystemParamsInput = z.input<typeof schemas.UpdateSystemParams>
export type WifiListInfo = z.output<typeof schemas.WifiListInfo>
export type ActionResult = z.output<typeof schemas.ActionResult>
export type AddSslOptions = z.output<typeof schemas.AddSslOptions>

/**
 * A completed background backup with its original target name and final report.
 */
export type BackupRun = z.output<typeof schemas.BackupRun>
export type BindingSetAddressEnabledParamsInput = z.input<
  typeof schemas.BindingSetAddressEnabledParams
>
export type BindingSetGuaWanParamsInput = z.input<
  typeof schemas.BindingSetGuaWanParams
>
export type DerivedAddressInfo = z.output<typeof schemas.DerivedAddressInfo>
export type DeviceInfo = z.output<typeof schemas.DeviceInfo>

/**
 * Database-generated patches contain only add, remove and replace operations.
 */
export type DiffPatch = z.output<typeof schemas.DiffPatch>
export type MetricsFollowResponse = z.output<
  typeof schemas.MetricsFollowResponse
>
export type NetworkGatewayInfo = z.output<typeof schemas.NetworkGatewayInfo>
export type SessionList = z.output<typeof schemas.SessionList>

/**
 * Parameters for replacing the stored NUT configuration.
 */
export type SetNutParamsInput = z.input<typeof schemas.SetNutParams>
export type BackupHistory = z.output<typeof schemas.BackupHistory>

/**
 * Active DNS authority and its bootstrap gateway.
 */
export type DnsStatus = z.output<typeof schemas.DnsStatus>
export type LegacyProxySslOptions = z.output<
  typeof schemas.LegacyProxySslOptions
>
export type Revision = z.output<typeof schemas.Revision>
export type CompatibleBindOptions = z.output<
  typeof schemas.CompatibleBindOptions
>

/**
 * A dump when `revisions` is null; otherwise the revisions since the cursor, current as of `id`.
 */
export type SubscribeStart = z.output<typeof schemas.SubscribeStart>
export type BindOptions = z.output<typeof schemas.BindOptions>

/**
 * Opted-in rpcstream requests receive the stream start directly; other callers receive a GUID.
 */
export type SubscribeStartResult = z.output<typeof schemas.SubscribeStartResult>
export type BindInfo = z.output<typeof schemas.BindInfo>
export type Bindings = z.output<typeof schemas.Bindings>
export type Api = RpcSchemaTree<typeof schemas.Api>
