import type { z } from 'zod'
import type * as schemas from './start-os.schemas'
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
export type AcmeProvider = z.output<typeof schemas.AcmeProvider>
export type AcmeProviderInput = z.input<typeof schemas.AcmeProvider>
export type ActionId = z.output<typeof schemas.ActionId>
export type ActionIdInput = z.input<typeof schemas.ActionId>
export type ActionResultV0 = z.output<typeof schemas.ActionResultV0>
export type ActionResultValue = z.output<typeof schemas.ActionResultValue>
export type AddVersionParamsInput = z.input<typeof schemas.AddVersionParams>
export type AnySignature = z.output<typeof schemas.AnySignature>
export type AnySignatureInput = z.input<typeof schemas.AnySignature>
export type AnyVerifyingKey = z.output<typeof schemas.AnyVerifyingKey>
export type AnyVerifyingKeyInput = z.input<typeof schemas.AnyVerifyingKey>
export type ApiState = z.output<typeof schemas.ApiState>
export type ApplyParamsInput = z.input<typeof schemas.ApplyParams>
export type BackupTargetIdInput = z.input<typeof schemas.BackupTargetId>
export type BasicCredential = z.output<typeof schemas.BasicCredential>
export type Blake3Commitment = z.output<typeof schemas.Blake3Commitment>
export type Blake3CommitmentInput = z.input<typeof schemas.Blake3Commitment>
export type CapabilityVerdict = z.output<typeof schemas.CapabilityVerdict>
export type Celsius = z.output<typeof schemas.Celsius>
export type CheckPortRes = z.output<typeof schemas.CheckPortRes>

/**
 * v6 reachability of the box's GUA at a port. v6 is NAT-free (the GUA is the
 * box's own address), so there is no hairpinning. Queried separately from
 * [`CheckPortRes`] so the IPv4 and IPv6 checks can run independently.
 */
export type CheckPortV6Res = z.output<typeof schemas.CheckPortV6Res>
export type CifsAddParamsInput = z.input<typeof schemas.CifsAddParams>
export type ContactInfo = z.output<typeof schemas.ContactInfo>
export type ContactInfoInput = z.input<typeof schemas.ContactInfo>
export type ContainerId = z.output<typeof schemas.ContainerId>
export type CountEntry = z.output<typeof schemas.CountEntry>
export type DataUrl = z.output<typeof schemas.DataUrl>
export type DataUrlInput = z.input<typeof schemas.DataUrl>
export type DeviceFilter = z.output<typeof schemas.DeviceFilter>
export type DnsSource = z.output<typeof schemas.DnsSource>
export type Dump = z.output<typeof schemas.Dump>
export type DumpParamsInput = z.input<typeof schemas.DumpParams>
export type EchoParamsInput = z.input<typeof schemas.EchoParams>
export type EncryptedWireInput = z.input<typeof schemas.EncryptedWire>

/**
 * Selects how aggressively an EPP-capable CPU pursues performance.
 */
export type Epp = z.output<typeof schemas.Epp>

/**
 * Selects how aggressively an EPP-capable CPU pursues performance.
 */
export type EppInput = z.input<typeof schemas.Epp>
export type FormatDiskParamsInput = z.input<typeof schemas.FormatDiskParams>
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
export type GetDownloadsParamsInput = z.input<typeof schemas.GetDownloadsParams>
export type GetOsAssetParamsInput = z.input<typeof schemas.GetOsAssetParams>
export type GetOsVersionParamsInput = z.input<typeof schemas.GetOsVersionParams>
export type GetUsersParamsInput = z.input<typeof schemas.GetUsersParams>
export type GigaBytes = z.output<typeof schemas.GigaBytes>
export type GitHash = z.output<typeof schemas.GitHash>
export type Governor = z.output<typeof schemas.Governor>
export type GovernorInput = z.input<typeof schemas.Governor>
export type Guid = z.output<typeof schemas.Guid>
export type GuidInput = z.input<typeof schemas.Guid>

/**
 * Proof that a set of auth keys was unenrolled — removed from the persisted
 * and ephemeral signer stores with any continuations they opened killed.
 * Obtained via [`SignatureAuthContext::unenroll`], or [`Self::unenroll`]
 * from inside a db transaction.
 */
export type HasUnenrolledKeys = z.output<typeof schemas.HasUnenrolledKeys>
export type HealthCheckId = z.output<typeof schemas.HealthCheckId>
export type HostId = z.output<typeof schemas.HostId>
export type HostIdInput = z.input<typeof schemas.HostId>
export type ImageIdInput = z.input<typeof schemas.ImageId>
export type IoFormatInput = z.input<typeof schemas.IoFormat>
export type KeyboardOptionsInput = z.input<typeof schemas.KeyboardOptions>
export type ListNotificationParamsInput = z.input<
  typeof schemas.ListNotificationParams
>
export type ListParamsInput = z.input<typeof schemas.ListParams>
export type ListVersionSignersParamsInput = z.input<
  typeof schemas.ListVersionSignersParams
>
export type LocaleString = z.output<typeof schemas.LocaleString>
export type LocaleStringInput = z.input<typeof schemas.LocaleString>
export type LogEntry = z.output<typeof schemas.LogEntry>
export type LogoutParamsInput = z.input<typeof schemas.LogoutParams>
export type LshwDisplay = z.output<typeof schemas.LshwDisplay>
export type LshwProcessor = z.output<typeof schemas.LshwProcessor>
export type MaybeUtf8String = z.output<typeof schemas.MaybeUtf8String>
export type MebiBytes = z.output<typeof schemas.MebiBytes>
export type MerkleArchiveCommitment = z.output<
  typeof schemas.MerkleArchiveCommitment
>
export type MerkleArchiveCommitmentInput = z.input<
  typeof schemas.MerkleArchiveCommitment
>
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
export type NetInfo = z.output<typeof schemas.NetInfo>
export type NetworkInterfaceType = z.output<typeof schemas.NetworkInterfaceType>
export type NotificationLevel = z.output<typeof schemas.NotificationLevel>
export type NotificationLevelInput = z.input<typeof schemas.NotificationLevel>

/**
 * An absent update preserves DNS intent; null servers select gateway DNS.
 */
export type OutboundDnsUpdateInput = z.input<typeof schemas.OutboundDnsUpdate>
export type PackageDetailLevelInput = z.input<typeof schemas.PackageDetailLevel>
export type PackageId = z.output<typeof schemas.PackageId>
export type PackageIdInput = z.input<typeof schemas.PackageId>
export type PackageVersionCount = z.output<typeof schemas.PackageVersionCount>
export type PartitionTable = z.output<typeof schemas.PartitionTable>
export type Percentage = z.output<typeof schemas.Percentage>
export type PluginId = z.output<typeof schemas.PluginId>
export type PreDownloadAlertWhen = z.output<typeof schemas.PreDownloadAlertWhen>
export type QueryDnsParamsInput = z.input<typeof schemas.QueryDnsParams>

/**
 * What a public domain currently resolves to, per address family. A public
 * domain is DualStack: the operator points an `A` record at the gateway's WAN
 * IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
 */
export type QueryDnsRes = z.output<typeof schemas.QueryDnsRes>

/**
 * A registry asset with a caller-selected commitment representation.
 */
export type RegistryAsset<Commitment> = z.output<
  ReturnType<typeof schemas.RegistryAsset<z.ZodType<Commitment, Commitment>>>
>
export type RegistryUrlParamsInput = z.input<typeof schemas.RegistryUrlParams>
export type RemoveAssetParamsInput = z.input<typeof schemas.RemoveAssetParams>
export type RemoveCategoryParamsInput = z.input<
  typeof schemas.RemoveCategoryParams
>
export type RemoveDomainParamsInput = z.input<typeof schemas.RemoveDomainParams>
export type RemovePassthroughParamsInput = z.input<
  typeof schemas.RemovePassthroughParams
>
export type RemoveVersionParamsInput = z.input<
  typeof schemas.RemoveVersionParams
>
export type ReplayIdInput = z.input<typeof schemas.ReplayId>
export type RequiresReboot = z.output<typeof schemas.RequiresReboot>
export type Security = z.output<typeof schemas.Security>
export type ServerHostname = z.output<typeof schemas.ServerHostname>
export type ServiceInterfaceId = z.output<typeof schemas.ServiceInterfaceId>
export type ServiceInterfaceType = z.output<typeof schemas.ServiceInterfaceType>
export type Session = z.output<typeof schemas.Session>
export type SetCountryParamsInput = z.input<typeof schemas.SetCountryParams>
export type SetEchoipUrlsParamsInput = z.input<
  typeof schemas.SetEchoipUrlsParams
>
export type SetLanguageParamsInput = z.input<typeof schemas.SetLanguageParams>
export type SetNameParamsInput = z.input<typeof schemas.SetNameParams>
export type SetServerHostnameParamsInput = z.input<
  typeof schemas.SetServerHostnameParams
>
export type SetWifiEnabledParamsInput = z.input<
  typeof schemas.SetWifiEnabledParams
>
export type ShutdownParamsInput = z.input<typeof schemas.ShutdownParams>
export type SideloadParamsInput = z.input<typeof schemas.SideloadParams>

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
export type SubscribeParamsInput = z.input<typeof schemas.SubscribeParams>
export type TermSizeInput = z.input<typeof schemas.TermSize>
export type TimeInfo = z.output<typeof schemas.TimeInfo>
export type TrustCaParamsInput = z.input<typeof schemas.TrustCaParams>
export type TrustedCa = z.output<typeof schemas.TrustedCa>
export type TrustedOsUpdateSigner = z.output<
  typeof schemas.TrustedOsUpdateSigner
>
export type UiParamsInput = z.input<typeof schemas.UiParams>

/**
 * How the OS reverse proxy validates the container's TLS certificate when it
 * rewraps SSL (`add_ssl` set AND `secure.ssl == true`, so the OS terminates
 * the client's TLS and initiates a fresh TLS connection to the container).
 * Absent (`None`) means validate against the StartOS root CA — the default.
 */
export type UpstreamCertValidation = z.output<
  typeof schemas.UpstreamCertValidation
>
export type UsersResponse = z.output<typeof schemas.UsersResponse>
export type Version = z.output<typeof schemas.Version>
export type VersionInput = z.input<typeof schemas.Version>
export type WifiAddParamsInput = z.input<typeof schemas.WifiAddParams>
export type WifiSsidParamsInput = z.input<typeof schemas.WifiSsidParams>
export type ZramParamsInput = z.input<typeof schemas.ZramParams>
export type AcceptSignersInput = z.input<typeof schemas.AcceptSigners>
export type ActionInput = z.output<typeof schemas.ActionInput>
export type ActionResultMember = z.output<typeof schemas.ActionResultMember>
export type ActionResultV1 = z.output<typeof schemas.ActionResultV1>
export type AddAdminParamsInput = z.input<typeof schemas.AddAdminParams>
export type AddAssetParamsInput = z.input<typeof schemas.AddAssetParams>
export type AddCategoryParamsInput = z.input<typeof schemas.AddCategoryParams>
export type AddMirrorParamsInput = z.input<typeof schemas.AddMirrorParams>
export type AddPackageParamsInput = z.input<typeof schemas.AddPackageParams>
export type AddPackageSignerParamsInput = z.input<
  typeof schemas.AddPackageSignerParams
>
export type AddPackageToCategoryParamsInput = z.input<
  typeof schemas.AddPackageToCategoryParams
>
export type AddPassthroughParamsInput = z.input<
  typeof schemas.AddPassthroughParams
>
export type AddPrivateDomainParamsInput = z.input<
  typeof schemas.AddPrivateDomainParams
>
export type AddPublicDomainParamsInput = z.input<
  typeof schemas.AddPublicDomainParams
>
export type AddTunnelParamsInput = z.input<typeof schemas.AddTunnelParams>
export type AddressInfo = z.output<typeof schemas.AddressInfo>

/**
 * The protocols a binding answers with, carried on the wire as the list itself.
 */
export type AlpnInfo = z.output<typeof schemas.AlpnInfo>
export type ApplyWithPathParamsInput = z.input<
  typeof schemas.ApplyWithPathParams
>
export type AttachParamsInput = z.input<typeof schemas.AttachParams>

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export type AuthKeys = z.output<typeof schemas.AuthKeys>
export type CancelInstallParamsInput = z.input<
  typeof schemas.CancelInstallParams
>
export type Category = z.output<typeof schemas.Category>
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
export type CurrentDependencyKind = z.output<
  typeof schemas.CurrentDependencyKind
>
export type DeleteLegacyParamsInput = z.input<typeof schemas.DeleteLegacyParams>
export type Description = z.output<typeof schemas.Description>

/**
 * Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted
 * override; `servers` and `source` are derived from it and the link configuration.
 * Without an override or gateway-provided resolvers, the public fallback is used.
 */
export type DnsSettings = z.output<typeof schemas.DnsSettings>
export type DownloadsResponse = z.output<typeof schemas.DownloadsResponse>
export type EditGatewayParamsInput = z.input<typeof schemas.EditGatewayParams>
export type EditSignerParamsInput = z.input<typeof schemas.EditSignerParams>
export type EppInfo = z.output<typeof schemas.EppInfo>
export type EppParamsInput = z.input<typeof schemas.EppParams>
export type ForgetGatewayParamsInput = z.input<
  typeof schemas.ForgetGatewayParams
>
export type ForwardTable = z.output<typeof schemas.ForwardTable>

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
export type GetPackageParamsInput = z.input<typeof schemas.GetPackageParams>
export type GovernorInfo = z.output<typeof schemas.GovernorInfo>
export type GovernorParamsInput = z.input<typeof schemas.GovernorParams>
export type HardwareRequirements = z.output<typeof schemas.HardwareRequirements>
export type HostnameMetadata = z.output<typeof schemas.HostnameMetadata>
export type HostnameMetadataInput = z.input<typeof schemas.HostnameMetadata>
export type InfoParamsInput = z.input<typeof schemas.InfoParams>
export type InitAcmeParamsInput = z.input<typeof schemas.InitAcmeParams>
export type InstallParamsInput = z.input<typeof schemas.InstallParams>
export type InstalledVersionParamsInput = z.input<
  typeof schemas.InstalledVersionParams
>
export type IpInfo = z.output<typeof schemas.IpInfo>
export type KillParamsInput = z.input<typeof schemas.KillParams>
export type LegacyAlpnInfo = z.output<typeof schemas.LegacyAlpnInfo>
export type ListPackageSignersParamsInput = z.input<
  typeof schemas.ListPackageSignersParams
>
export type LogFollowResponse = z.output<typeof schemas.LogFollowResponse>
export type LogResponse = z.output<typeof schemas.LogResponse>
export type LoginParamsInput = z.input<typeof schemas.LoginParams>
export type LshwDevice = z.output<typeof schemas.LshwDevice>
export type MetricsCpu = z.output<typeof schemas.MetricsCpu>
export type MetricsDisk = z.output<typeof schemas.MetricsDisk>
export type MetricsGeneral = z.output<typeof schemas.MetricsGeneral>
export type MetricsMemory = z.output<typeof schemas.MetricsMemory>
export type MetricsSummary = z.output<typeof schemas.MetricsSummary>
export type MountParamsInput = z.input<typeof schemas.MountParams>
export type Notification = z.output<typeof schemas.Notification>
export type OsInfo = z.output<typeof schemas.OsInfo>
export type OsUpdateSignerKeyParamsInput = z.input<
  typeof schemas.OsUpdateSignerKeyParams
>
export type OsVersionInfo = z.output<typeof schemas.OsVersionInfo>
export type PackageBackupInfo = z.output<typeof schemas.PackageBackupInfo>

/**
 * Inline for StartOS before 0.4.0.3 and from registries before 1.2.0.
 */
export type PackageIcon = z.output<typeof schemas.PackageIcon>
export type PackageIdParamsInput = z.input<typeof schemas.PackageIdParams>
export type PackageInfoShort = z.output<typeof schemas.PackageInfoShort>
export type PassthroughInfo = z.output<typeof schemas.PassthroughInfo>
export type PasswordTypeInput = z.input<typeof schemas.PasswordType>
export type PreDownloadAlert = z.output<typeof schemas.PreDownloadAlert>

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
export type RebuildParamsInput = z.input<typeof schemas.RebuildParams>
export type RemoveAcmeParamsInput = z.input<typeof schemas.RemoveAcmeParams>
export type RemoveAdminParamsInput = z.input<typeof schemas.RemoveAdminParams>
export type RemoveMirrorParamsInput = z.input<typeof schemas.RemoveMirrorParams>
export type RemovePackageFromCategoryParamsInput = z.input<
  typeof schemas.RemovePackageFromCategoryParams
>
export type RemovePackageParamsInput = z.input<
  typeof schemas.RemovePackageParams
>
export type RemovePackageSignerParamsInput = z.input<
  typeof schemas.RemovePackageSignerParams
>
export type RemoveSignerParamsInput = z.input<typeof schemas.RemoveSignerParams>
export type RemoveTunnelParamsInput = z.input<typeof schemas.RemoveTunnelParams>
export type RenameGatewayParamsInput = z.input<
  typeof schemas.RenameGatewayParams
>
export type RequiresHostIdInput = z.input<typeof schemas.RequiresHostId>
export type RequiresPackageIdInput = z.input<typeof schemas.RequiresPackageId>
export type RestorePackageParamsInput = z.input<
  typeof schemas.RestorePackageParams
>
export type RunActionParamsInput = z.input<typeof schemas.RunActionParams>
export type ServiceStats = z.output<typeof schemas.ServiceStats>
export type SetDefaultOutboundParamsInput = z.input<
  typeof schemas.SetDefaultOutboundParams
>
export type SetDescriptionParamsInput = z.input<
  typeof schemas.SetDescriptionParams
>
export type SetGatewaySecureParamsInput = z.input<
  typeof schemas.SetGatewaySecureParams
>
export type SetIconParamsInput = z.input<typeof schemas.SetIconParams>
export type SetOsUpdateSignerParamsInput = z.input<
  typeof schemas.SetOsUpdateSignerParams
>
export type SetOutboundGatewayParamsInput = z.input<
  typeof schemas.SetOutboundGatewayParams
>
export type SetStaticDnsParamsInput = z.input<typeof schemas.SetStaticDnsParams>
export type SideloadResponse = z.output<typeof schemas.SideloadResponse>
export type SignAssetParamsInput = z.input<typeof schemas.SignAssetParams>
export type SignerInfo = z.output<typeof schemas.SignerInfo>
export type SignerInfoInput = z.input<typeof schemas.SignerInfo>
export type SmtpValueInput = z.input<typeof schemas.SmtpValue>
export type SshAddParamsInput = z.input<typeof schemas.SshAddParams>

/**
 * The public view of a backup found on a target: enough to identify it, and none of
 * [`BackupUnencryptedMetadata`]'s key material.
 */
export type StartOsRecoveryInfo = z.output<typeof schemas.StartOsRecoveryInfo>
export type SubscribeRes = z.output<typeof schemas.SubscribeRes>
export type TestSmtpParamsInput = z.input<typeof schemas.TestSmtpParams>
export type UmountParamsInput = z.input<typeof schemas.UmountParams>
export type UninstallParamsInput = z.input<typeof schemas.UninstallParams>
export type UnsetGatewaySecureParamsInput = z.input<
  typeof schemas.UnsetGatewaySecureParams
>
export type UpdateSystemRes = z.output<typeof schemas.UpdateSystemRes>
export type UpdateTunnelParamsInput = z.input<typeof schemas.UpdateTunnelParams>
export type VersionSignerParamsInput = z.input<
  typeof schemas.VersionSignerParams
>
export type WifiListOut = z.output<typeof schemas.WifiListOut>
export type ActionResult = z.output<typeof schemas.ActionResult>
export type AddPublicDomainRes = z.output<typeof schemas.AddPublicDomainRes>
export type BackupInfo = z.output<typeof schemas.BackupInfo>
export type BackupParamsInput = z.input<typeof schemas.BackupParams>
export type CifsBackupTarget = z.output<typeof schemas.CifsBackupTarget>
export type CompatibleAlpnInfo = z.output<typeof schemas.CompatibleAlpnInfo>
export type DependencyMetadata = z.output<typeof schemas.DependencyMetadata>
export type HardwareInfo = z.output<typeof schemas.HardwareInfo>
export type HostAddress = z.output<typeof schemas.HostAddress>
export type HostnameInfo = z.output<typeof schemas.HostnameInfo>
export type HostnameInfoInput = z.input<typeof schemas.HostnameInfo>
export type Metrics = z.output<typeof schemas.Metrics>
export type NetworkInterfaceInfo = z.output<typeof schemas.NetworkInterfaceInfo>
export type NotificationWithId = z.output<typeof schemas.NotificationWithId>
export type OsVersionInfoMap = z.output<typeof schemas.OsVersionInfoMap>
export type PackageMetadata = z.output<typeof schemas.PackageMetadata>
export type PartitionInfo = z.output<typeof schemas.PartitionInfo>
export type RegistryInfo = z.output<typeof schemas.RegistryInfo>
export type ResetPasswordParamsInput = z.input<
  typeof schemas.ResetPasswordParams
>
export type ServiceInterface = z.output<typeof schemas.ServiceInterface>
export type SessionList = z.output<typeof schemas.SessionList>
export type UpdateSystemParamsInput = z.input<typeof schemas.UpdateSystemParams>
export type WifiListInfo = z.output<typeof schemas.WifiListInfo>
export type AddSslOptions = z.output<typeof schemas.AddSslOptions>
export type BackupTarget = z.output<typeof schemas.BackupTarget>
export type BindingSetAddressEnabledParamsInput = z.input<
  typeof schemas.BindingSetAddressEnabledParams
>
export type BindingSetGuaWanParamsInput = z.input<
  typeof schemas.BindingSetGuaWanParams
>
export type DerivedAddressInfo = z.output<typeof schemas.DerivedAddressInfo>
export type DeviceInfo = z.output<typeof schemas.DeviceInfo>
export type DiskInfo = z.output<typeof schemas.DiskInfo>
export type MetricsFollowResponse = z.output<
  typeof schemas.MetricsFollowResponse
>
export type NetworkGatewayInfo = z.output<typeof schemas.NetworkGatewayInfo>
export type OsIndex = z.output<typeof schemas.OsIndex>
export type PackageVersionInfo = z.output<typeof schemas.PackageVersionInfo>
export type BindOptions = z.output<typeof schemas.BindOptions>

/**
 * Active DNS authority and its bootstrap gateway.
 */
export type DnsStatus = z.output<typeof schemas.DnsStatus>
export type GetPackageResponse = z.output<typeof schemas.GetPackageResponse>
export type GetPackageResponseFull = z.output<
  typeof schemas.GetPackageResponseFull
>
export type PackageInfo = z.output<typeof schemas.PackageInfo>
export type BindInfo = z.output<typeof schemas.BindInfo>
export type GetPackageResult = z.output<typeof schemas.GetPackageResult>
export type PackageIndex = z.output<typeof schemas.PackageIndex>
export type Bindings = z.output<typeof schemas.Bindings>
export type FullIndex = z.output<typeof schemas.FullIndex>
export type Api = RpcSchemaTree<typeof schemas.Api>
