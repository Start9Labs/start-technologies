import type { z } from 'zod'
import type * as schemas from './types.schemas'
export type AcmeProvider = z.output<typeof schemas.AcmeProvider>
export type AcmeProviderInput = z.input<typeof schemas.AcmeProvider>
export type AcmeSettings = z.output<typeof schemas.AcmeSettings>
export type AcmeSettingsInput = z.input<typeof schemas.AcmeSettings>
export type ActionAccess = z.output<typeof schemas.ActionAccess>
export type ActionAccessInput = z.input<typeof schemas.ActionAccess>
export type ActionId = z.output<typeof schemas.ActionId>
export type ActionIdInput = z.input<typeof schemas.ActionId>
export type ActionResultV0 = z.output<typeof schemas.ActionResultV0>
export type ActionResultV0Input = z.input<typeof schemas.ActionResultV0>
export type ActionResultValue = z.output<typeof schemas.ActionResultValue>
export type ActionResultValueInput = z.input<typeof schemas.ActionResultValue>
export type ActionVisibility = z.output<typeof schemas.ActionVisibility>
export type ActionVisibilityInput = z.input<typeof schemas.ActionVisibility>
export type Algorithm = z.output<typeof schemas.Algorithm>
export type AlgorithmInput = z.input<typeof schemas.Algorithm>
export type AllowedStatuses = z.output<typeof schemas.AllowedStatuses>
export type AllowedStatusesInput = z.input<typeof schemas.AllowedStatuses>
export type AnySignature = z.output<typeof schemas.AnySignature>
export type AnySignatureInput = z.input<typeof schemas.AnySignature>
export type AnySigningKey = z.output<typeof schemas.AnySigningKey>
export type AnySigningKeyInput = z.input<typeof schemas.AnySigningKey>
export type AnyVerifyingKey = z.output<typeof schemas.AnyVerifyingKey>
export type AnyVerifyingKeyInput = z.input<typeof schemas.AnyVerifyingKey>
export type ApiState = z.output<typeof schemas.ApiState>
export type ApiStateInput = z.input<typeof schemas.ApiState>
export type BackupTargetId = z.output<typeof schemas.BackupTargetId>
export type BackupTargetIdInput = z.input<typeof schemas.BackupTargetId>
export type Base64 = z.output<typeof schemas.Base64>
export type Base64Input = z.input<typeof schemas.Base64>
export type BasicCredential = z.output<typeof schemas.BasicCredential>
export type BasicCredentialInput = z.input<typeof schemas.BasicCredential>
export type Blake3Commitment = z.output<typeof schemas.Blake3Commitment>
export type Blake3CommitmentInput = z.input<typeof schemas.Blake3Commitment>
export type BlockDev = z.output<typeof schemas.BlockDev>
export type BlockDevInput = z.input<typeof schemas.BlockDev>
export type BuildArg = z.output<typeof schemas.BuildArg>
export type BuildArgInput = z.input<typeof schemas.BuildArg>
export type CallbackId = z.output<typeof schemas.CallbackId>
export type CallbackIdInput = z.input<typeof schemas.CallbackId>
export type CapabilityVerdict = z.output<typeof schemas.CapabilityVerdict>
export type CapabilityVerdictInput = z.input<typeof schemas.CapabilityVerdict>
export type Celsius = z.output<typeof schemas.Celsius>
export type CelsiusInput = z.input<typeof schemas.Celsius>
export type CheckPortRes = z.output<typeof schemas.CheckPortRes>
export type CheckPortResInput = z.input<typeof schemas.CheckPortRes>

/**
 * v6 reachability of the box's GUA at a port. v6 is NAT-free (the GUA is the
 * box's own address), so there is no hairpinning. Queried separately from
 * [`CheckPortRes`] so the IPv4 and IPv6 checks can run independently.
 */
export type CheckPortV6Res = z.output<typeof schemas.CheckPortV6Res>

/**
 * v6 reachability of the box's GUA at a port. v6 is NAT-free (the GUA is the
 * box's own address), so there is no hairpinning. Queried separately from
 * [`CheckPortRes`] so the IPv4 and IPv6 checks can run independently.
 */
export type CheckPortV6ResInput = z.input<typeof schemas.CheckPortV6Res>
export type Cifs = z.output<typeof schemas.Cifs>
export type CifsInput = z.input<typeof schemas.Cifs>
export type CifsAddParams = z.output<typeof schemas.CifsAddParams>
export type CifsAddParamsInput = z.input<typeof schemas.CifsAddParams>
export type ClearCallbacksParams = z.output<typeof schemas.ClearCallbacksParams>
export type ClearCallbacksParamsInput = z.input<
  typeof schemas.ClearCallbacksParams
>
export type ClearTasksParams = z.output<typeof schemas.ClearTasksParams>
export type ClearTasksParamsInput = z.input<typeof schemas.ClearTasksParams>
export type CliSetIconParams = z.output<typeof schemas.CliSetIconParams>
export type CliSetIconParamsInput = z.input<typeof schemas.CliSetIconParams>
export type ContactInfo = z.output<typeof schemas.ContactInfo>
export type ContactInfoInput = z.input<typeof schemas.ContactInfo>
export type CountEntry = z.output<typeof schemas.CountEntry>
export type CountEntryInput = z.input<typeof schemas.CountEntry>
export type CreateTokenParams = z.output<typeof schemas.CreateTokenParams>
export type CreateTokenParamsInput = z.input<typeof schemas.CreateTokenParams>
export type CreatedToken = z.output<typeof schemas.CreatedToken>
export type CreatedTokenInput = z.input<typeof schemas.CreatedToken>
export type DataUrl = z.output<typeof schemas.DataUrl>
export type DataUrlInput = z.input<typeof schemas.DataUrl>
export type DeviceFilter = z.output<typeof schemas.DeviceFilter>
export type DeviceFilterInput = z.input<typeof schemas.DeviceFilter>
export type DnsSource = z.output<typeof schemas.DnsSource>
export type DnsSourceInput = z.input<typeof schemas.DnsSource>
export type Duration = z.output<typeof schemas.Duration>
export type DurationInput = z.input<typeof schemas.Duration>
export type EchoParams = z.output<typeof schemas.EchoParams>
export type EchoParamsInput = z.input<typeof schemas.EchoParams>
export type EncryptedWire = z.output<typeof schemas.EncryptedWire>
export type EncryptedWireInput = z.input<typeof schemas.EncryptedWire>

/**
 * Selects how aggressively an EPP-capable CPU pursues performance.
 */
export type Epp = z.output<typeof schemas.Epp>

/**
 * Selects how aggressively an EPP-capable CPU pursues performance.
 */
export type EppInput = z.input<typeof schemas.Epp>
export type ErrorData = z.output<typeof schemas.ErrorData>
export type ErrorDataInput = z.input<typeof schemas.ErrorData>
export type FileType = z.output<typeof schemas.FileType>
export type FileTypeInput = z.input<typeof schemas.FileType>
export type GatewayId = z.output<typeof schemas.GatewayId>
export type GatewayIdInput = z.input<typeof schemas.GatewayId>
export type GatewayType = z.output<typeof schemas.GatewayType>
export type GatewayTypeInput = z.input<typeof schemas.GatewayType>
export type GenerateCertificateParams = z.output<
  typeof schemas.GenerateCertificateParams
>
export type GenerateCertificateParamsInput = z.input<
  typeof schemas.GenerateCertificateParams
>
export type GenerateCertificateResponse = z.output<
  typeof schemas.GenerateCertificateResponse
>
export type GenerateCertificateResponseInput = z.input<
  typeof schemas.GenerateCertificateResponse
>
export type GetDownloadsParams = z.output<typeof schemas.GetDownloadsParams>
export type GetDownloadsParamsInput = z.input<typeof schemas.GetDownloadsParams>
export type GetOsAssetParams = z.output<typeof schemas.GetOsAssetParams>
export type GetOsAssetParamsInput = z.input<typeof schemas.GetOsAssetParams>
export type GetOsVersionParams = z.output<typeof schemas.GetOsVersionParams>
export type GetOsVersionParamsInput = z.input<typeof schemas.GetOsVersionParams>
export type GetUsersParams = z.output<typeof schemas.GetUsersParams>
export type GetUsersParamsInput = z.input<typeof schemas.GetUsersParams>
export type GigaBytes = z.output<typeof schemas.GigaBytes>
export type GigaBytesInput = z.input<typeof schemas.GigaBytes>
export type GitHash = z.output<typeof schemas.GitHash>
export type GitHashInput = z.input<typeof schemas.GitHash>
export type Governor = z.output<typeof schemas.Governor>
export type GovernorInput = z.input<typeof schemas.Governor>
export type Guid = z.output<typeof schemas.Guid>
export type GuidInput = z.input<typeof schemas.Guid>
export type HealthCheckId = z.output<typeof schemas.HealthCheckId>
export type HealthCheckIdInput = z.input<typeof schemas.HealthCheckId>
export type HostId = z.output<typeof schemas.HostId>
export type HostIdInput = z.input<typeof schemas.HostId>
export type IdMap = z.output<typeof schemas.IdMap>
export type IdMapInput = z.input<typeof schemas.IdMap>
export type ImageId = z.output<typeof schemas.ImageId>
export type ImageIdInput = z.input<typeof schemas.ImageId>
export type ImageMetadata = z.output<typeof schemas.ImageMetadata>
export type ImageMetadataInput = z.input<typeof schemas.ImageMetadata>
export type KeyboardOptions = z.output<typeof schemas.KeyboardOptions>
export type KeyboardOptionsInput = z.input<typeof schemas.KeyboardOptions>
export type LegacyHealthCheckResultKind = z.output<
  typeof schemas.LegacyHealthCheckResultKind
>
export type LegacyHealthCheckResultKindInput = z.input<
  typeof schemas.LegacyHealthCheckResultKind
>
export type ListNotificationParams = z.output<
  typeof schemas.ListNotificationParams
>
export type ListNotificationParamsInput = z.input<
  typeof schemas.ListNotificationParams
>
export type ListVersionSignersParams = z.output<
  typeof schemas.ListVersionSignersParams
>
export type ListVersionSignersParamsInput = z.input<
  typeof schemas.ListVersionSignersParams
>
export type LocaleString = z.output<typeof schemas.LocaleString>
export type LocaleStringInput = z.input<typeof schemas.LocaleString>
export type LogEntry = z.output<typeof schemas.LogEntry>
export type LogEntryInput = z.input<typeof schemas.LogEntry>
export type LogsParams = z.output<typeof schemas.LogsParams>
export type LogsParamsInput = z.input<typeof schemas.LogsParams>
export type LshwDisplay = z.output<typeof schemas.LshwDisplay>
export type LshwDisplayInput = z.input<typeof schemas.LshwDisplay>
export type LshwProcessor = z.output<typeof schemas.LshwProcessor>
export type LshwProcessorInput = z.input<typeof schemas.LshwProcessor>
export type MaybeUtf8String = z.output<typeof schemas.MaybeUtf8String>
export type MaybeUtf8StringInput = z.input<typeof schemas.MaybeUtf8String>
export type MebiBytes = z.output<typeof schemas.MebiBytes>
export type MebiBytesInput = z.input<typeof schemas.MebiBytes>
export type MerkleArchiveCommitment = z.output<
  typeof schemas.MerkleArchiveCommitment
>
export type MerkleArchiveCommitmentInput = z.input<
  typeof schemas.MerkleArchiveCommitment
>
export type ModifyNotificationBeforeParams = z.output<
  typeof schemas.ModifyNotificationBeforeParams
>
export type ModifyNotificationBeforeParamsInput = z.input<
  typeof schemas.ModifyNotificationBeforeParams
>
export type ModifyNotificationParams = z.output<
  typeof schemas.ModifyNotificationParams
>
export type ModifyNotificationParamsInput = z.input<
  typeof schemas.ModifyNotificationParams
>
export type NamedHealthCheckResultKind = z.output<
  typeof schemas.NamedHealthCheckResultKind
>
export type NamedHealthCheckResultKindInput = z.input<
  typeof schemas.NamedHealthCheckResultKind
>
export type NamedProgress = z.output<typeof schemas.NamedProgress>
export type NamedProgressInput = z.input<typeof schemas.NamedProgress>
export type NetInfo = z.output<typeof schemas.NetInfo>
export type NetInfoInput = z.input<typeof schemas.NetInfo>
export type NetworkInterfaceType = z.output<typeof schemas.NetworkInterfaceType>
export type NetworkInterfaceTypeInput = z.input<
  typeof schemas.NetworkInterfaceType
>
export type NotificationLevel = z.output<typeof schemas.NotificationLevel>
export type NotificationLevelInput = z.input<typeof schemas.NotificationLevel>

/**
 * An absent update preserves DNS intent; null servers select gateway DNS.
 */
export type OutboundDnsUpdate = z.output<typeof schemas.OutboundDnsUpdate>

/**
 * An absent update preserves DNS intent; null servers select gateway DNS.
 */
export type OutboundDnsUpdateInput = z.input<typeof schemas.OutboundDnsUpdate>
export type PackageBackupReport = z.output<typeof schemas.PackageBackupReport>
export type PackageBackupReportInput = z.input<
  typeof schemas.PackageBackupReport
>
export type PackageDetailLevel = z.output<typeof schemas.PackageDetailLevel>
export type PackageDetailLevelInput = z.input<typeof schemas.PackageDetailLevel>
export type PackageId = z.output<typeof schemas.PackageId>
export type PackageIdInput = z.input<typeof schemas.PackageId>
export type PackageVersionCount = z.output<typeof schemas.PackageVersionCount>
export type PackageVersionCountInput = z.input<
  typeof schemas.PackageVersionCount
>
export type PathOrUrl = z.output<typeof schemas.PathOrUrl>
export type PathOrUrlInput = z.input<typeof schemas.PathOrUrl>
export type Pem = z.output<typeof schemas.Pem>
export type PemInput = z.input<typeof schemas.Pem>
export type Percentage = z.output<typeof schemas.Percentage>
export type PercentageInput = z.input<typeof schemas.Percentage>
export type PluginId = z.output<typeof schemas.PluginId>
export type PluginIdInput = z.input<typeof schemas.PluginId>
export type PreDownloadAlertWhen = z.output<typeof schemas.PreDownloadAlertWhen>
export type PreDownloadAlertWhenInput = z.input<
  typeof schemas.PreDownloadAlertWhen
>
export type ProgressUnits = z.output<typeof schemas.ProgressUnits>
export type ProgressUnitsInput = z.input<typeof schemas.ProgressUnits>
export type QueryDnsParams = z.output<typeof schemas.QueryDnsParams>
export type QueryDnsParamsInput = z.input<typeof schemas.QueryDnsParams>

/**
 * What a public domain currently resolves to, per address family. A public
 * domain is DualStack: the operator points an `A` record at the gateway's WAN
 * IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
 */
export type QueryDnsRes = z.output<typeof schemas.QueryDnsRes>

/**
 * What a public domain currently resolves to, per address family. A public
 * domain is DualStack: the operator points an `A` record at the gateway's WAN
 * IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
 */
export type QueryDnsResInput = z.input<typeof schemas.QueryDnsRes>

/**
 * A migration or backup recovery source with a caller-selected backup password representation.
 */
export type RecoverySource<Password> = z.output<
  ReturnType<typeof schemas.RecoverySource<z.ZodType<Password, Password>>>
>

/**
 * A migration or backup recovery source with a caller-selected backup password representation.
 */
export type RecoverySourceInput<Password> = z.input<
  ReturnType<typeof schemas.RecoverySource<z.ZodType<Password, Password>>>
>

/**
 * A registry asset with a caller-selected commitment representation.
 */
export type RegistryAsset<Commitment> = z.output<
  ReturnType<typeof schemas.RegistryAsset<z.ZodType<Commitment, Commitment>>>
>

/**
 * A registry asset with a caller-selected commitment representation.
 */
export type RegistryAssetInput<Commitment> = z.input<
  ReturnType<typeof schemas.RegistryAsset<z.ZodType<Commitment, Commitment>>>
>
export type RemoveAssetParams = z.output<typeof schemas.RemoveAssetParams>
export type RemoveAssetParamsInput = z.input<typeof schemas.RemoveAssetParams>
export type RemoveCategoryParams = z.output<typeof schemas.RemoveCategoryParams>
export type RemoveCategoryParamsInput = z.input<
  typeof schemas.RemoveCategoryParams
>
export type RemoveDomainParams = z.output<typeof schemas.RemoveDomainParams>
export type RemoveDomainParamsInput = z.input<typeof schemas.RemoveDomainParams>
export type RemoveVersionParams = z.output<typeof schemas.RemoveVersionParams>
export type RemoveVersionParamsInput = z.input<
  typeof schemas.RemoveVersionParams
>
export type ReplayId = z.output<typeof schemas.ReplayId>
export type ReplayIdInput = z.input<typeof schemas.ReplayId>
export type RequestCommitment = z.output<typeof schemas.RequestCommitment>
export type RequestCommitmentInput = z.input<typeof schemas.RequestCommitment>
export type RestartReason = z.output<typeof schemas.RestartReason>
export type RestartReasonInput = z.input<typeof schemas.RestartReason>
export type Security = z.output<typeof schemas.Security>
export type SecurityInput = z.input<typeof schemas.Security>
export type ServerBackupReport = z.output<typeof schemas.ServerBackupReport>
export type ServerBackupReportInput = z.input<typeof schemas.ServerBackupReport>
export type ServerHostname = z.output<typeof schemas.ServerHostname>
export type ServerHostnameInput = z.input<typeof schemas.ServerHostname>
export type ServerSpecs = z.output<typeof schemas.ServerSpecs>
export type ServerSpecsInput = z.input<typeof schemas.ServerSpecs>
export type ServiceInterfaceId = z.output<typeof schemas.ServiceInterfaceId>
export type ServiceInterfaceIdInput = z.input<typeof schemas.ServiceInterfaceId>
export type ServiceInterfaceType = z.output<typeof schemas.ServiceInterfaceType>
export type ServiceInterfaceTypeInput = z.input<
  typeof schemas.ServiceInterfaceType
>
export type Session = z.output<typeof schemas.Session>
export type SessionInput = z.input<typeof schemas.Session>
export type SetCountryParams = z.output<typeof schemas.SetCountryParams>
export type SetCountryParamsInput = z.input<typeof schemas.SetCountryParams>
export type SetDataVersionParams = z.output<typeof schemas.SetDataVersionParams>
export type SetDataVersionParamsInput = z.input<
  typeof schemas.SetDataVersionParams
>
export type SetLanguageParams = z.output<typeof schemas.SetLanguageParams>
export type SetLanguageParamsInput = z.input<typeof schemas.SetLanguageParams>
export type SetMainStatusStatus = z.output<typeof schemas.SetMainStatusStatus>
export type SetMainStatusStatusInput = z.input<
  typeof schemas.SetMainStatusStatus
>
export type SetNameParams = z.output<typeof schemas.SetNameParams>
export type SetNameParamsInput = z.input<typeof schemas.SetNameParams>
export type SetServerHostnameParams = z.output<
  typeof schemas.SetServerHostnameParams
>
export type SetServerHostnameParamsInput = z.input<
  typeof schemas.SetServerHostnameParams
>
export type SetWifiEnabledParams = z.output<typeof schemas.SetWifiEnabledParams>
export type SetWifiEnabledParamsInput = z.input<
  typeof schemas.SetWifiEnabledParams
>
export type SetupInfo = z.output<typeof schemas.SetupInfo>
export type SetupInfoInput = z.input<typeof schemas.SetupInfo>
export type ShutdownParams = z.output<typeof schemas.ShutdownParams>
export type ShutdownParamsInput = z.input<typeof schemas.ShutdownParams>
export type SideloadParams = z.output<typeof schemas.SideloadParams>
export type SideloadParamsInput = z.input<typeof schemas.SideloadParams>

/**
 * So a signal strength is a number between 0-100, I want the null option to be 0 since there is no signal
 */
export type SignalStrength = z.output<typeof schemas.SignalStrength>

/**
 * So a signal strength is a number between 0-100, I want the null option to be 0 since there is no signal
 */
export type SignalStrengthInput = z.input<typeof schemas.SignalStrength>
export type SmtpSecurity = z.output<typeof schemas.SmtpSecurity>
export type SmtpSecurityInput = z.input<typeof schemas.SmtpSecurity>
export type SshDeleteParams = z.output<typeof schemas.SshDeleteParams>
export type SshDeleteParamsInput = z.input<typeof schemas.SshDeleteParams>
export type SshKeyResponse = z.output<typeof schemas.SshKeyResponse>
export type SshKeyResponseInput = z.input<typeof schemas.SshKeyResponse>
export type SshPubKey = z.output<typeof schemas.SshPubKey>
export type SshPubKeyInput = z.input<typeof schemas.SshPubKey>

/**
 * Ssid are the names of the wifis, usually human readable.
 */
export type Ssid = z.output<typeof schemas.Ssid>

/**
 * Ssid are the names of the wifis, usually human readable.
 */
export type SsidInput = z.input<typeof schemas.Ssid>
export type StartStop = z.output<typeof schemas.StartStop>
export type StartStopInput = z.input<typeof schemas.StartStop>
export type TaskCondition = z.output<typeof schemas.TaskCondition>
export type TaskConditionInput = z.input<typeof schemas.TaskCondition>
export type TaskInputRepr = z.output<typeof schemas.TaskInputRepr>
export type TaskInputReprInput = z.input<typeof schemas.TaskInputRepr>
export type TaskSeverity = z.output<typeof schemas.TaskSeverity>
export type TaskSeverityInput = z.input<typeof schemas.TaskSeverity>
export type TimeInfo = z.output<typeof schemas.TimeInfo>
export type TimeInfoInput = z.input<typeof schemas.TimeInfo>
export type TrustedOsUpdateSigner = z.output<
  typeof schemas.TrustedOsUpdateSigner
>
export type TrustedOsUpdateSignerInput = z.input<
  typeof schemas.TrustedOsUpdateSigner
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

/**
 * How the OS reverse proxy validates the container's TLS certificate when it
 * rewraps SSL (`add_ssl` set AND `secure.ssl == true`, so the OS terminates
 * the client's TLS and initiates a fresh TLS connection to the container).
 * Absent (`None`) means validate against the StartOS root CA — the default.
 */
export type UpstreamCertValidationInput = z.input<
  typeof schemas.UpstreamCertValidation
>
export type UsersResponse = z.output<typeof schemas.UsersResponse>
export type UsersResponseInput = z.input<typeof schemas.UsersResponse>
export type Version = z.output<typeof schemas.Version>
export type VersionInput = z.input<typeof schemas.Version>
export type VolumeId = z.output<typeof schemas.VolumeId>
export type VolumeIdInput = z.input<typeof schemas.VolumeId>
export type WifiAddParams = z.output<typeof schemas.WifiAddParams>
export type WifiAddParamsInput = z.input<typeof schemas.WifiAddParams>
export type WifiSsidParams = z.output<typeof schemas.WifiSsidParams>
export type WifiSsidParamsInput = z.input<typeof schemas.WifiSsidParams>
export type AcceptSigners = z.output<typeof schemas.AcceptSigners>
export type AcceptSignersInput = z.input<typeof schemas.AcceptSigners>
export type ActionInput = z.output<typeof schemas.ActionInput>
export type ActionInputInput = z.input<typeof schemas.ActionInput>
export type ActionMetadata = z.output<typeof schemas.ActionMetadata>
export type ActionMetadataInput = z.input<typeof schemas.ActionMetadata>
export type ActionResultMember = z.output<typeof schemas.ActionResultMember>
export type ActionResultMemberInput = z.input<typeof schemas.ActionResultMember>
export type ActionResultV1 = z.output<typeof schemas.ActionResultV1>
export type ActionResultV1Input = z.input<typeof schemas.ActionResultV1>
export type AddAdminParams = z.output<typeof schemas.AddAdminParams>
export type AddAdminParamsInput = z.input<typeof schemas.AddAdminParams>
export type AddAssetParams = z.output<typeof schemas.AddAssetParams>
export type AddAssetParamsInput = z.input<typeof schemas.AddAssetParams>
export type AddCategoryParams = z.output<typeof schemas.AddCategoryParams>
export type AddCategoryParamsInput = z.input<typeof schemas.AddCategoryParams>
export type AddMirrorParams = z.output<typeof schemas.AddMirrorParams>
export type AddMirrorParamsInput = z.input<typeof schemas.AddMirrorParams>
export type AddPackageParams = z.output<typeof schemas.AddPackageParams>
export type AddPackageParamsInput = z.input<typeof schemas.AddPackageParams>
export type AddPackageSignerParams = z.output<
  typeof schemas.AddPackageSignerParams
>
export type AddPackageSignerParamsInput = z.input<
  typeof schemas.AddPackageSignerParams
>
export type AddPackageToCategoryParams = z.output<
  typeof schemas.AddPackageToCategoryParams
>
export type AddPackageToCategoryParamsInput = z.input<
  typeof schemas.AddPackageToCategoryParams
>
export type AddPrivateDomainParams = z.output<
  typeof schemas.AddPrivateDomainParams
>
export type AddPrivateDomainParamsInput = z.input<
  typeof schemas.AddPrivateDomainParams
>
export type AddPublicDomainParams = z.output<
  typeof schemas.AddPublicDomainParams
>
export type AddPublicDomainParamsInput = z.input<
  typeof schemas.AddPublicDomainParams
>
export type AddTunnelParams = z.output<typeof schemas.AddTunnelParams>
export type AddTunnelParamsInput = z.input<typeof schemas.AddTunnelParams>
export type AddVersionParams = z.output<typeof schemas.AddVersionParams>
export type AddVersionParamsInput = z.input<typeof schemas.AddVersionParams>
export type AddressInfo = z.output<typeof schemas.AddressInfo>
export type AddressInfoInput = z.input<typeof schemas.AddressInfo>

/**
 * The protocols a binding answers with, carried on the wire as the list itself.
 */
export type AlpnInfo = z.output<typeof schemas.AlpnInfo>

/**
 * The protocols a binding answers with, carried on the wire as the list itself.
 */
export type AlpnInfoInput = z.input<typeof schemas.AlpnInfo>
export type AttachParams = z.output<typeof schemas.AttachParams>
export type AttachParamsInput = z.input<typeof schemas.AttachParams>

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export type AuthKeys = z.output<typeof schemas.AuthKeys>

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export type AuthKeysInput = z.input<typeof schemas.AuthKeys>
export type BackupReport = z.output<typeof schemas.BackupReport>
export type BackupReportInput = z.input<typeof schemas.BackupReport>
export type BackupTargetFS = z.output<typeof schemas.BackupTargetFS>
export type BackupTargetFSInput = z.input<typeof schemas.BackupTargetFS>
export type BindId = z.output<typeof schemas.BindId>
export type BindIdInput = z.input<typeof schemas.BindId>
export type BindRangeParams = z.output<typeof schemas.BindRangeParams>
export type BindRangeParamsInput = z.input<typeof schemas.BindRangeParams>
export type CancelInstallParams = z.output<typeof schemas.CancelInstallParams>
export type CancelInstallParamsInput = z.input<
  typeof schemas.CancelInstallParams
>
export type Category = z.output<typeof schemas.Category>
export type CategoryInput = z.input<typeof schemas.Category>
export type CheckChallengeParams = z.output<typeof schemas.CheckChallengeParams>
export type CheckChallengeParamsInput = z.input<
  typeof schemas.CheckChallengeParams
>

/**
 * Reachability of the port a certificate authority validates on. A null leg
 * went unprobed.
 */
export type CheckChallengeRes = z.output<typeof schemas.CheckChallengeRes>

/**
 * Reachability of the port a certificate authority validates on. A null leg
 * went unprobed.
 */
export type CheckChallengeResInput = z.input<typeof schemas.CheckChallengeRes>
export type CheckDependenciesParam = z.output<
  typeof schemas.CheckDependenciesParam
>
export type CheckDependenciesParamInput = z.input<
  typeof schemas.CheckDependenciesParam
>
export type CheckDnsParams = z.output<typeof schemas.CheckDnsParams>
export type CheckDnsParamsInput = z.input<typeof schemas.CheckDnsParams>
export type CheckPortParams = z.output<typeof schemas.CheckPortParams>
export type CheckPortParamsInput = z.input<typeof schemas.CheckPortParams>
export type CifsRemoveParams = z.output<typeof schemas.CifsRemoveParams>
export type CifsRemoveParamsInput = z.input<typeof schemas.CifsRemoveParams>
export type CifsUpdateParams = z.output<typeof schemas.CifsUpdateParams>
export type CifsUpdateParamsInput = z.input<typeof schemas.CifsUpdateParams>
export type ClearActionsParams = z.output<typeof schemas.ClearActionsParams>
export type ClearActionsParamsInput = z.input<typeof schemas.ClearActionsParams>
export type ClearServiceInterfacesParams = z.output<
  typeof schemas.ClearServiceInterfacesParams
>
export type ClearServiceInterfacesParamsInput = z.input<
  typeof schemas.ClearServiceInterfacesParams
>
export type ClearTaskParams = z.output<typeof schemas.ClearTaskParams>
export type ClearTaskParamsInput = z.input<typeof schemas.ClearTaskParams>
export type CliSetDescriptionParams = z.output<
  typeof schemas.CliSetDescriptionParams
>
export type CliSetDescriptionParamsInput = z.input<
  typeof schemas.CliSetDescriptionParams
>
export type CompatibleHealthCheckResultKind = z.output<
  typeof schemas.CompatibleHealthCheckResultKind
>
export type CompatibleHealthCheckResultKindInput = z.input<
  typeof schemas.CompatibleHealthCheckResultKind
>
export type ControlParams = z.output<typeof schemas.ControlParams>
export type ControlParamsInput = z.input<typeof schemas.ControlParams>
export type CreateNotificationParams = z.output<
  typeof schemas.CreateNotificationParams
>
export type CreateNotificationParamsInput = z.input<
  typeof schemas.CreateNotificationParams
>
export type CreateSubcontainerFsParams = z.output<
  typeof schemas.CreateSubcontainerFsParams
>
export type CreateSubcontainerFsParamsInput = z.input<
  typeof schemas.CreateSubcontainerFsParams
>
export type CurrentDependencyKind = z.output<
  typeof schemas.CurrentDependencyKind
>
export type CurrentDependencyKindInput = z.input<
  typeof schemas.CurrentDependencyKind
>
export type DeleteLegacyParams = z.output<typeof schemas.DeleteLegacyParams>
export type DeleteLegacyParamsInput = z.input<typeof schemas.DeleteLegacyParams>
export type DependencyRequirement = z.output<
  typeof schemas.DependencyRequirement
>
export type DependencyRequirementInput = z.input<
  typeof schemas.DependencyRequirement
>
export type Description = z.output<typeof schemas.Description>
export type DescriptionInput = z.input<typeof schemas.Description>
export type DesiredStatus = z.output<typeof schemas.DesiredStatus>
export type DesiredStatusInput = z.input<typeof schemas.DesiredStatus>
export type DestroySubcontainerFsParams = z.output<
  typeof schemas.DestroySubcontainerFsParams
>
export type DestroySubcontainerFsParamsInput = z.input<
  typeof schemas.DestroySubcontainerFsParams
>

/**
 * Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted
 * override; `servers` and `source` are derived from it and the link configuration.
 * Without an override or gateway-provided resolvers, the public fallback is used.
 */
export type DnsSettings = z.output<typeof schemas.DnsSettings>

/**
 * Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted
 * override; `servers` and `source` are derived from it and the link configuration.
 * Without an override or gateway-provided resolvers, the public fallback is used.
 */
export type DnsSettingsInput = z.input<typeof schemas.DnsSettings>
export type DomainSettings = z.output<typeof schemas.DomainSettings>
export type DomainSettingsInput = z.input<typeof schemas.DomainSettings>
export type DownloadsResponse = z.output<typeof schemas.DownloadsResponse>
export type DownloadsResponseInput = z.input<typeof schemas.DownloadsResponse>
export type EditGatewayParams = z.output<typeof schemas.EditGatewayParams>
export type EditGatewayParamsInput = z.input<typeof schemas.EditGatewayParams>
export type EditSignerParams = z.output<typeof schemas.EditSignerParams>
export type EditSignerParamsInput = z.input<typeof schemas.EditSignerParams>
export type EffectsRunActionParams = z.output<
  typeof schemas.EffectsRunActionParams
>
export type EffectsRunActionParamsInput = z.input<
  typeof schemas.EffectsRunActionParams
>
export type ExportRangeServiceInterfaceParams = z.output<
  typeof schemas.ExportRangeServiceInterfaceParams
>
export type ExportRangeServiceInterfaceParamsInput = z.input<
  typeof schemas.ExportRangeServiceInterfaceParams
>
export type ForgetGatewayParams = z.output<typeof schemas.ForgetGatewayParams>
export type ForgetGatewayParamsInput = z.input<
  typeof schemas.ForgetGatewayParams
>
export type FullProgress = z.output<typeof schemas.FullProgress>
export type FullProgressInput = z.input<typeof schemas.FullProgress>
export type GatewayInfo = z.output<typeof schemas.GatewayInfo>
export type GatewayInfoInput = z.input<typeof schemas.GatewayInfo>

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

/**
 * Whether the gateway reachable via this interface speaks each port-mapping
 * protocol, as last probed. Fed by the watcher's periodic probes and by
 * failure/success evidence from the port-map client, and synced to the db so a
 * chronically uncooperative gateway is visible (and skipped) instead of being
 * retried forever.
 */
export type GatewayPortMapCapabilitiesInput = z.input<
  typeof schemas.GatewayPortMapCapabilities
>
export type GetActionInputParams = z.output<typeof schemas.GetActionInputParams>
export type GetActionInputParamsInput = z.input<
  typeof schemas.GetActionInputParams
>
export type GetContainerIpParams = z.output<typeof schemas.GetContainerIpParams>
export type GetContainerIpParamsInput = z.input<
  typeof schemas.GetContainerIpParams
>
export type GetHostInfoParams = z.output<typeof schemas.GetHostInfoParams>
export type GetHostInfoParamsInput = z.input<typeof schemas.GetHostInfoParams>
export type GetOutboundGatewayParams = z.output<
  typeof schemas.GetOutboundGatewayParams
>
export type GetOutboundGatewayParamsInput = z.input<
  typeof schemas.GetOutboundGatewayParams
>
export type GetPackageParams = z.output<typeof schemas.GetPackageParams>
export type GetPackageParamsInput = z.input<typeof schemas.GetPackageParams>
export type GetServiceInterfaceParams = z.output<
  typeof schemas.GetServiceInterfaceParams
>
export type GetServiceInterfaceParamsInput = z.input<
  typeof schemas.GetServiceInterfaceParams
>
export type GetServiceManifestParams = z.output<
  typeof schemas.GetServiceManifestParams
>
export type GetServiceManifestParamsInput = z.input<
  typeof schemas.GetServiceManifestParams
>
export type GetServicePortForwardParams = z.output<
  typeof schemas.GetServicePortForwardParams
>
export type GetServicePortForwardParamsInput = z.input<
  typeof schemas.GetServicePortForwardParams
>
export type GetSslCertificateParams = z.output<
  typeof schemas.GetSslCertificateParams
>
export type GetSslCertificateParamsInput = z.input<
  typeof schemas.GetSslCertificateParams
>
export type GetSslKeyParams = z.output<typeof schemas.GetSslKeyParams>
export type GetSslKeyParamsInput = z.input<typeof schemas.GetSslKeyParams>
export type GetStatusParams = z.output<typeof schemas.GetStatusParams>
export type GetStatusParamsInput = z.input<typeof schemas.GetStatusParams>
export type GetSystemSmtpParams = z.output<typeof schemas.GetSystemSmtpParams>
export type GetSystemSmtpParamsInput = z.input<
  typeof schemas.GetSystemSmtpParams
>
export type HardwareRequirements = z.output<typeof schemas.HardwareRequirements>
export type HardwareRequirementsInput = z.input<
  typeof schemas.HardwareRequirements
>
export type HostnameMetadata = z.output<typeof schemas.HostnameMetadata>
export type HostnameMetadataInput = z.input<typeof schemas.HostnameMetadata>
export type ImageSource = z.output<typeof schemas.ImageSource>
export type ImageSourceInput = z.input<typeof schemas.ImageSource>
export type InfoParams = z.output<typeof schemas.InfoParams>
export type InfoParamsInput = z.input<typeof schemas.InfoParams>
export type InitAcmeParams = z.output<typeof schemas.InitAcmeParams>
export type InitAcmeParamsInput = z.input<typeof schemas.InitAcmeParams>
export type InstallParams = z.output<typeof schemas.InstallParams>
export type InstallParamsInput = z.input<typeof schemas.InstallParams>
export type InstalledVersionParams = z.output<
  typeof schemas.InstalledVersionParams
>
export type InstalledVersionParamsInput = z.input<
  typeof schemas.InstalledVersionParams
>
export type IpInfo = z.output<typeof schemas.IpInfo>
export type IpInfoInput = z.input<typeof schemas.IpInfo>
export type KillParams = z.output<typeof schemas.KillParams>
export type KillParamsInput = z.input<typeof schemas.KillParams>
export type LegacyAlpnInfo = z.output<typeof schemas.LegacyAlpnInfo>
export type LegacyAlpnInfoInput = z.input<typeof schemas.LegacyAlpnInfo>
export type ListPackageSignersParams = z.output<
  typeof schemas.ListPackageSignersParams
>
export type ListPackageSignersParamsInput = z.input<
  typeof schemas.ListPackageSignersParams
>
export type ListServiceInterfacesParams = z.output<
  typeof schemas.ListServiceInterfacesParams
>
export type ListServiceInterfacesParamsInput = z.input<
  typeof schemas.ListServiceInterfacesParams
>
export type LogFollowResponse = z.output<typeof schemas.LogFollowResponse>
export type LogFollowResponseInput = z.input<typeof schemas.LogFollowResponse>
export type LogResponse = z.output<typeof schemas.LogResponse>
export type LogResponseInput = z.input<typeof schemas.LogResponse>
export type LoginParams = z.output<typeof schemas.LoginParams>
export type LoginParamsInput = z.input<typeof schemas.LoginParams>
export type LshwDevice = z.output<typeof schemas.LshwDevice>
export type LshwDeviceInput = z.input<typeof schemas.LshwDevice>
export type Metadata = z.output<typeof schemas.Metadata>
export type MetadataInput = z.input<typeof schemas.Metadata>
export type MetricsCpu = z.output<typeof schemas.MetricsCpu>
export type MetricsCpuInput = z.input<typeof schemas.MetricsCpu>
export type MetricsDisk = z.output<typeof schemas.MetricsDisk>
export type MetricsDiskInput = z.input<typeof schemas.MetricsDisk>
export type MetricsGeneral = z.output<typeof schemas.MetricsGeneral>
export type MetricsGeneralInput = z.input<typeof schemas.MetricsGeneral>
export type MetricsMemory = z.output<typeof schemas.MetricsMemory>
export type MetricsMemoryInput = z.input<typeof schemas.MetricsMemory>
export type MetricsSummary = z.output<typeof schemas.MetricsSummary>
export type MetricsSummaryInput = z.input<typeof schemas.MetricsSummary>
export type MountTarget = z.output<typeof schemas.MountTarget>
export type MountTargetInput = z.input<typeof schemas.MountTarget>
export type Notification = z.output<typeof schemas.Notification>
export type NotificationInput = z.input<typeof schemas.Notification>
export type OsUpdateSignerKeyParams = z.output<
  typeof schemas.OsUpdateSignerKeyParams
>
export type OsUpdateSignerKeyParamsInput = z.input<
  typeof schemas.OsUpdateSignerKeyParams
>
export type OsVersionInfo = z.output<typeof schemas.OsVersionInfo>
export type OsVersionInfoInput = z.input<typeof schemas.OsVersionInfo>
export type PackageBackupInfo = z.output<typeof schemas.PackageBackupInfo>
export type PackageBackupInfoInput = z.input<typeof schemas.PackageBackupInfo>

/**
 * Inline for StartOS before 0.4.0.3 and from registries before 1.2.0.
 */
export type PackageIcon = z.output<typeof schemas.PackageIcon>

/**
 * Inline for StartOS before 0.4.0.3 and from registries before 1.2.0.
 */
export type PackageIconInput = z.input<typeof schemas.PackageIcon>
export type PackageInfoShort = z.output<typeof schemas.PackageInfoShort>
export type PackageInfoShortInput = z.input<typeof schemas.PackageInfoShort>
export type PassthroughInfo = z.output<typeof schemas.PassthroughInfo>
export type PassthroughInfoInput = z.input<typeof schemas.PassthroughInfo>
export type PasswordType = z.output<typeof schemas.PasswordType>
export type PasswordTypeInput = z.input<typeof schemas.PasswordType>
export type PluginHostnameInfo = z.output<typeof schemas.PluginHostnameInfo>
export type PluginHostnameInfoInput = z.input<typeof schemas.PluginHostnameInfo>
export type PortForward = z.output<typeof schemas.PortForward>
export type PortForwardInput = z.input<typeof schemas.PortForward>
export type PreDownloadAlert = z.output<typeof schemas.PreDownloadAlert>
export type PreDownloadAlertInput = z.input<typeof schemas.PreDownloadAlert>

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
export type ProxyAuthInput = z.input<typeof schemas.ProxyAuth>
export type PublicDomainConfig = z.output<typeof schemas.PublicDomainConfig>
export type PublicDomainConfigInput = z.input<typeof schemas.PublicDomainConfig>

/**
 * The single restricted service interface a port-range binding may export.
 *
 * Unlike [`ServiceInterface`], a range interface is always `api`-typed and
 * carries no `masked` / `username` / `path` / `query` and no per-address
 * [`AddressInfo`] — its address is the host plus the range's external port
 * span, taken from the [`RangeBindInfo`](crate::net::host::binding::RangeBindInfo)
 * it lives under. `scheme` is an optional transport prefix (e.g. `tcp` for
 * bitcoin ZMQ endpoints); most ranges (coturn RTP, FTP data) omit it.
 */
export type RangeServiceInterface = z.output<
  typeof schemas.RangeServiceInterface
>

/**
 * The single restricted service interface a port-range binding may export.
 *
 * Unlike [`ServiceInterface`], a range interface is always `api`-typed and
 * carries no `masked` / `username` / `path` / `query` and no per-address
 * [`AddressInfo`] — its address is the host plus the range's external port
 * span, taken from the [`RangeBindInfo`](crate::net::host::binding::RangeBindInfo)
 * it lives under. `scheme` is an optional transport prefix (e.g. `tcp` for
 * bitcoin ZMQ endpoints); most ranges (coturn RTP, FTP data) omit it.
 */
export type RangeServiceInterfaceInput = z.input<
  typeof schemas.RangeServiceInterface
>
export type RebuildParams = z.output<typeof schemas.RebuildParams>
export type RebuildParamsInput = z.input<typeof schemas.RebuildParams>
export type RemoveAcmeParams = z.output<typeof schemas.RemoveAcmeParams>
export type RemoveAcmeParamsInput = z.input<typeof schemas.RemoveAcmeParams>
export type RemoveAdminParams = z.output<typeof schemas.RemoveAdminParams>
export type RemoveAdminParamsInput = z.input<typeof schemas.RemoveAdminParams>
export type RemoveMirrorParams = z.output<typeof schemas.RemoveMirrorParams>
export type RemoveMirrorParamsInput = z.input<typeof schemas.RemoveMirrorParams>
export type RemovePackageFromCategoryParams = z.output<
  typeof schemas.RemovePackageFromCategoryParams
>
export type RemovePackageFromCategoryParamsInput = z.input<
  typeof schemas.RemovePackageFromCategoryParams
>
export type RemovePackageParams = z.output<typeof schemas.RemovePackageParams>
export type RemovePackageParamsInput = z.input<
  typeof schemas.RemovePackageParams
>
export type RemovePackageSignerParams = z.output<
  typeof schemas.RemovePackageSignerParams
>
export type RemovePackageSignerParamsInput = z.input<
  typeof schemas.RemovePackageSignerParams
>
export type RemoveSignerParams = z.output<typeof schemas.RemoveSignerParams>
export type RemoveSignerParamsInput = z.input<typeof schemas.RemoveSignerParams>
export type RemoveTunnelParams = z.output<typeof schemas.RemoveTunnelParams>
export type RemoveTunnelParamsInput = z.input<typeof schemas.RemoveTunnelParams>
export type RenameGatewayParams = z.output<typeof schemas.RenameGatewayParams>
export type RenameGatewayParamsInput = z.input<
  typeof schemas.RenameGatewayParams
>
export type RestorePackageParams = z.output<typeof schemas.RestorePackageParams>
export type RestorePackageParamsInput = z.input<
  typeof schemas.RestorePackageParams
>
export type RetireBindingParams = z.output<typeof schemas.RetireBindingParams>
export type RetireBindingParamsInput = z.input<
  typeof schemas.RetireBindingParams
>
export type RetireHostParams = z.output<typeof schemas.RetireHostParams>
export type RetireHostParamsInput = z.input<typeof schemas.RetireHostParams>
export type RunActionParams = z.output<typeof schemas.RunActionParams>
export type RunActionParamsInput = z.input<typeof schemas.RunActionParams>
export type ServiceDependencyMetadata = z.output<
  typeof schemas.ServiceDependencyMetadata
>
export type ServiceDependencyMetadataInput = z.input<
  typeof schemas.ServiceDependencyMetadata
>
export type SetDefaultOutboundParams = z.output<
  typeof schemas.SetDefaultOutboundParams
>
export type SetDefaultOutboundParamsInput = z.input<
  typeof schemas.SetDefaultOutboundParams
>
export type SetDescriptionParams = z.output<typeof schemas.SetDescriptionParams>
export type SetDescriptionParamsInput = z.input<
  typeof schemas.SetDescriptionParams
>
export type SetGatewaySecureParams = z.output<
  typeof schemas.SetGatewaySecureParams
>
export type SetGatewaySecureParamsInput = z.input<
  typeof schemas.SetGatewaySecureParams
>
export type SetIconParams = z.output<typeof schemas.SetIconParams>
export type SetIconParamsInput = z.input<typeof schemas.SetIconParams>
export type SetMainStatus = z.output<typeof schemas.SetMainStatus>
export type SetMainStatusInput = z.input<typeof schemas.SetMainStatus>
export type SetOsUpdateSignerParams = z.output<
  typeof schemas.SetOsUpdateSignerParams
>
export type SetOsUpdateSignerParamsInput = z.input<
  typeof schemas.SetOsUpdateSignerParams
>
export type SetOutboundGatewayParams = z.output<
  typeof schemas.SetOutboundGatewayParams
>
export type SetOutboundGatewayParamsInput = z.input<
  typeof schemas.SetOutboundGatewayParams
>
export type SetStaticDnsParams = z.output<typeof schemas.SetStaticDnsParams>
export type SetStaticDnsParamsInput = z.input<typeof schemas.SetStaticDnsParams>
export type SetupExecuteParams = z.output<typeof schemas.SetupExecuteParams>
export type SetupExecuteParamsInput = z.input<typeof schemas.SetupExecuteParams>
export type SetupResult = z.output<typeof schemas.SetupResult>
export type SetupResultInput = z.input<typeof schemas.SetupResult>
export type SideloadResponse = z.output<typeof schemas.SideloadResponse>
export type SideloadResponseInput = z.input<typeof schemas.SideloadResponse>
export type SignAssetParams = z.output<typeof schemas.SignAssetParams>
export type SignAssetParamsInput = z.input<typeof schemas.SignAssetParams>
export type SignerInfo = z.output<typeof schemas.SignerInfo>
export type SignerInfoInput = z.input<typeof schemas.SignerInfo>
export type SmtpValue = z.output<typeof schemas.SmtpValue>
export type SmtpValueInput = z.input<typeof schemas.SmtpValue>
export type SshAddParams = z.output<typeof schemas.SshAddParams>
export type SshAddParamsInput = z.input<typeof schemas.SshAddParams>

/**
 * The public view of a backup found on a target: enough to identify it, and none of
 * [`BackupUnencryptedMetadata`]'s key material.
 */
export type StartOsRecoveryInfo = z.output<typeof schemas.StartOsRecoveryInfo>

/**
 * The public view of a backup found on a target: enough to identify it, and none of
 * [`BackupUnencryptedMetadata`]'s key material.
 */
export type StartOsRecoveryInfoInput = z.input<
  typeof schemas.StartOsRecoveryInfo
>
export type TaskInput = z.output<typeof schemas.TaskInput>
export type TaskInputInput = z.input<typeof schemas.TaskInput>
export type TaskTrigger = z.output<typeof schemas.TaskTrigger>
export type TaskTriggerInput = z.input<typeof schemas.TaskTrigger>
export type TestSmtpParams = z.output<typeof schemas.TestSmtpParams>
export type TestSmtpParamsInput = z.input<typeof schemas.TestSmtpParams>
export type UmountParams = z.output<typeof schemas.UmountParams>
export type UmountParamsInput = z.input<typeof schemas.UmountParams>
export type UninstallParams = z.output<typeof schemas.UninstallParams>
export type UninstallParamsInput = z.input<typeof schemas.UninstallParams>
export type UnsetGatewaySecureParams = z.output<
  typeof schemas.UnsetGatewaySecureParams
>
export type UnsetGatewaySecureParamsInput = z.input<
  typeof schemas.UnsetGatewaySecureParams
>
export type UpdateSystemRes = z.output<typeof schemas.UpdateSystemRes>
export type UpdateSystemResInput = z.input<typeof schemas.UpdateSystemRes>
export type UpdateTunnelParams = z.output<typeof schemas.UpdateTunnelParams>
export type UpdateTunnelParamsInput = z.input<typeof schemas.UpdateTunnelParams>
export type UrlPluginRegisterParams = z.output<
  typeof schemas.UrlPluginRegisterParams
>
export type UrlPluginRegisterParamsInput = z.input<
  typeof schemas.UrlPluginRegisterParams
>
export type UrlPluginRegistration = z.output<
  typeof schemas.UrlPluginRegistration
>
export type UrlPluginRegistrationInput = z.input<
  typeof schemas.UrlPluginRegistration
>
export type VerifyCifsParams = z.output<typeof schemas.VerifyCifsParams>
export type VerifyCifsParamsInput = z.input<typeof schemas.VerifyCifsParams>
export type VersionSignerParams = z.output<typeof schemas.VersionSignerParams>
export type VersionSignerParamsInput = z.input<
  typeof schemas.VersionSignerParams
>
export type WifiInfo = z.output<typeof schemas.WifiInfo>
export type WifiInfoInput = z.input<typeof schemas.WifiInfo>
export type WifiListOut = z.output<typeof schemas.WifiListOut>
export type WifiListOutInput = z.input<typeof schemas.WifiListOut>
export type ActionResult = z.output<typeof schemas.ActionResult>
export type ActionResultInput = z.input<typeof schemas.ActionResult>
export type AddPublicDomainRes = z.output<typeof schemas.AddPublicDomainRes>
export type AddPublicDomainResInput = z.input<typeof schemas.AddPublicDomainRes>
export type BackupInfo = z.output<typeof schemas.BackupInfo>
export type BackupInfoInput = z.input<typeof schemas.BackupInfo>
export type BackupParams = z.output<typeof schemas.BackupParams>
export type BackupParamsInput = z.input<typeof schemas.BackupParams>
export type CifsBackupTarget = z.output<typeof schemas.CifsBackupTarget>
export type CifsBackupTargetInput = z.input<typeof schemas.CifsBackupTarget>
export type ClearBindingsParams = z.output<typeof schemas.ClearBindingsParams>
export type ClearBindingsParamsInput = z.input<
  typeof schemas.ClearBindingsParams
>
export type CompatibleAlpnInfo = z.output<typeof schemas.CompatibleAlpnInfo>
export type CompatibleAlpnInfoInput = z.input<typeof schemas.CompatibleAlpnInfo>
export type CurrentDependencyInfo = z.output<
  typeof schemas.CurrentDependencyInfo
>
export type CurrentDependencyInfoInput = z.input<
  typeof schemas.CurrentDependencyInfo
>
export type DependencyMetadata = z.output<typeof schemas.DependencyMetadata>
export type DependencyMetadataInput = z.input<typeof schemas.DependencyMetadata>
export type ExportActionParams = z.output<typeof schemas.ExportActionParams>
export type ExportActionParamsInput = z.input<typeof schemas.ExportActionParams>
export type ExportServiceInterfaceParams = z.output<
  typeof schemas.ExportServiceInterfaceParams
>
export type ExportServiceInterfaceParamsInput = z.input<
  typeof schemas.ExportServiceInterfaceParams
>
export type HostnameInfo = z.output<typeof schemas.HostnameInfo>
export type HostnameInfoInput = z.input<typeof schemas.HostnameInfo>
export type ImageConfigRepr = z.output<typeof schemas.ImageConfigRepr>
export type ImageConfigReprInput = z.input<typeof schemas.ImageConfigRepr>
export type InitProgressRes = z.output<typeof schemas.InitProgressRes>
export type InitProgressResInput = z.input<typeof schemas.InitProgressRes>
export type MetadataSrc = z.output<typeof schemas.MetadataSrc>
export type MetadataSrcInput = z.input<typeof schemas.MetadataSrc>
export type Metrics = z.output<typeof schemas.Metrics>
export type MetricsInput = z.input<typeof schemas.Metrics>
export type MountParams = z.output<typeof schemas.MountParams>
export type MountParamsInput = z.input<typeof schemas.MountParams>
export type NamedHealthCheckResult = z.output<
  typeof schemas.NamedHealthCheckResult
>
export type NamedHealthCheckResultInput = z.input<
  typeof schemas.NamedHealthCheckResult
>
export type NetworkInterfaceInfo = z.output<typeof schemas.NetworkInterfaceInfo>
export type NetworkInterfaceInfoInput = z.input<
  typeof schemas.NetworkInterfaceInfo
>
export type NotificationWithId = z.output<typeof schemas.NotificationWithId>
export type NotificationWithIdInput = z.input<typeof schemas.NotificationWithId>
export type OsVersionInfoMap = z.output<typeof schemas.OsVersionInfoMap>
export type OsVersionInfoMapInput = z.input<typeof schemas.OsVersionInfoMap>
export type PackageMetadata = z.output<typeof schemas.PackageMetadata>
export type PackageMetadataInput = z.input<typeof schemas.PackageMetadata>
export type PackagePlugin = z.output<typeof schemas.PackagePlugin>
export type PackagePluginInput = z.input<typeof schemas.PackagePlugin>
export type PartitionInfo = z.output<typeof schemas.PartitionInfo>
export type PartitionInfoInput = z.input<typeof schemas.PartitionInfo>
export type Progress = z.output<typeof schemas.Progress>
export type ProgressInput = z.input<typeof schemas.Progress>
export type ProxyOptions = z.output<typeof schemas.ProxyOptions>
export type ProxyOptionsInput = z.input<typeof schemas.ProxyOptions>
export type RegistryInfo = z.output<typeof schemas.RegistryInfo>
export type RegistryInfoInput = z.input<typeof schemas.RegistryInfo>
export type ResetPasswordParams = z.output<typeof schemas.ResetPasswordParams>
export type ResetPasswordParamsInput = z.input<
  typeof schemas.ResetPasswordParams
>
export type ServerStatus = z.output<typeof schemas.ServerStatus>
export type ServerStatusInput = z.input<typeof schemas.ServerStatus>
export type ServiceInterface = z.output<typeof schemas.ServiceInterface>
export type ServiceInterfaceInput = z.input<typeof schemas.ServiceInterface>

/**
 * The server's sessions, by credential type.
 */
export type Sessions = z.output<typeof schemas.Sessions>

/**
 * The server's sessions, by credential type.
 */
export type SessionsInput = z.input<typeof schemas.Sessions>
export type SetDependenciesParams = z.output<
  typeof schemas.SetDependenciesParams
>
export type SetDependenciesParamsInput = z.input<
  typeof schemas.SetDependenciesParams
>
export type SetupProgress = z.output<typeof schemas.SetupProgress>
export type SetupProgressInput = z.input<typeof schemas.SetupProgress>
export type Task = z.output<typeof schemas.Task>
export type TaskParams = z.input<typeof schemas.Task>
export type UpdateSystemParams = z.output<typeof schemas.UpdateSystemParams>
export type UpdateSystemParamsInput = z.input<typeof schemas.UpdateSystemParams>
export type UrlPluginClearUrlsParams = z.output<
  typeof schemas.UrlPluginClearUrlsParams
>
export type UrlPluginClearUrlsParamsInput = z.input<
  typeof schemas.UrlPluginClearUrlsParams
>
export type UrlPluginExportUrlParams = z.output<
  typeof schemas.UrlPluginExportUrlParams
>
export type UrlPluginExportUrlParamsInput = z.input<
  typeof schemas.UrlPluginExportUrlParams
>
export type WifiListInfo = z.output<typeof schemas.WifiListInfo>
export type WifiListInfoInput = z.input<typeof schemas.WifiListInfo>
export type AddSslOptions = z.output<typeof schemas.AddSslOptions>
export type AddSslOptionsInput = z.input<typeof schemas.AddSslOptions>
export type BackupTarget = z.output<typeof schemas.BackupTarget>
export type BackupTargetInput = z.input<typeof schemas.BackupTarget>
export type BindingSetAddressEnabledParams = z.output<
  typeof schemas.BindingSetAddressEnabledParams
>
export type BindingSetAddressEnabledParamsInput = z.input<
  typeof schemas.BindingSetAddressEnabledParams
>
export type BindingSetGuaWanParams = z.output<
  typeof schemas.BindingSetGuaWanParams
>
export type BindingSetGuaWanParamsInput = z.input<
  typeof schemas.BindingSetGuaWanParams
>
export type CreateTaskParams = z.output<typeof schemas.CreateTaskParams>
export type CreateTaskParamsInput = z.input<typeof schemas.CreateTaskParams>
export type CurrentDependencies = z.output<typeof schemas.CurrentDependencies>
export type CurrentDependenciesInput = z.input<
  typeof schemas.CurrentDependencies
>
export type DepInfo = z.output<typeof schemas.DepInfo>
export type DepInfoInput = z.input<typeof schemas.DepInfo>
export type DerivedAddressInfo = z.output<typeof schemas.DerivedAddressInfo>
export type DerivedAddressInfoInput = z.input<typeof schemas.DerivedAddressInfo>
export type ImageConfig = z.output<typeof schemas.ImageConfig>
export type ImageConfigInput = z.input<typeof schemas.ImageConfig>
export type MetricsFollowResponse = z.output<
  typeof schemas.MetricsFollowResponse
>
export type MetricsFollowResponseInput = z.input<
  typeof schemas.MetricsFollowResponse
>
export type NetworkGatewayInfo = z.output<typeof schemas.NetworkGatewayInfo>
export type NetworkGatewayInfoInput = z.input<typeof schemas.NetworkGatewayInfo>
export type OsIndex = z.output<typeof schemas.OsIndex>
export type OsIndexInput = z.input<typeof schemas.OsIndex>
export type PackageVersionInfo = z.output<typeof schemas.PackageVersionInfo>
export type PackageVersionInfoInput = z.input<typeof schemas.PackageVersionInfo>
export type SessionList = z.output<typeof schemas.SessionList>
export type SessionListInput = z.input<typeof schemas.SessionList>
export type SetBackupProgress = z.output<typeof schemas.SetBackupProgress>
export type SetBackupProgressInput = z.input<typeof schemas.SetBackupProgress>
export type SetHealth = z.output<typeof schemas.SetHealth>
export type SetHealthInput = z.input<typeof schemas.SetHealth>
export type SetInitProgress = z.output<typeof schemas.SetInitProgress>
export type SetInitProgressInput = z.input<typeof schemas.SetInitProgress>
export type SetupStatusRes = z.output<typeof schemas.SetupStatusRes>
export type SetupStatusResInput = z.input<typeof schemas.SetupStatusRes>
export type StatusInfo = z.output<typeof schemas.StatusInfo>
export type StatusInfoInput = z.input<typeof schemas.StatusInfo>
export type TaskEntry = z.output<typeof schemas.TaskEntry>
export type TaskEntryInput = z.input<typeof schemas.TaskEntry>
export type CheckDependenciesResult = z.output<
  typeof schemas.CheckDependenciesResult
>
export type CheckDependenciesResultInput = z.input<
  typeof schemas.CheckDependenciesResult
>
export type Dependencies = z.output<typeof schemas.Dependencies>
export type DependenciesInput = z.input<typeof schemas.Dependencies>

/**
 * Active DNS authority and its bootstrap gateway.
 */
export type DnsStatus = z.output<typeof schemas.DnsStatus>

/**
 * Active DNS authority and its bootstrap gateway.
 */
export type DnsStatusInput = z.input<typeof schemas.DnsStatus>
export type GetPackageResponse = z.output<typeof schemas.GetPackageResponse>
export type GetPackageResponseInput = z.input<typeof schemas.GetPackageResponse>
export type GetPackageResponseFull = z.output<
  typeof schemas.GetPackageResponseFull
>
export type GetPackageResponseFullInput = z.input<
  typeof schemas.GetPackageResponseFull
>
export type LegacyProxySslOptions = z.output<
  typeof schemas.LegacyProxySslOptions
>
export type LegacyProxySslOptionsInput = z.input<
  typeof schemas.LegacyProxySslOptions
>
export type PackageInfo = z.output<typeof schemas.PackageInfo>
export type PackageInfoInput = z.input<typeof schemas.PackageInfo>

/**
 * Contiguous port-range binding (e.g. WebRTC/STUN/TURN RTP ranges).
 *
 * Keyed by `internal_start_port` in [`BindingRanges`]. The range covers
 * `internal_start_port..(internal_start_port + number_of_ports)` and is
 * forwarded through a single iptables rule per protocol per gateway,
 * preserving the destination port number.
 */
export type RangeBindInfo = z.output<typeof schemas.RangeBindInfo>

/**
 * Contiguous port-range binding (e.g. WebRTC/STUN/TURN RTP ranges).
 *
 * Keyed by `internal_start_port` in [`BindingRanges`]. The range covers
 * `internal_start_port..(internal_start_port + number_of_ports)` and is
 * forwarded through a single iptables rule per protocol per gateway,
 * preserving the destination port number.
 */
export type RangeBindInfoInput = z.input<typeof schemas.RangeBindInfo>
export type BindingRanges = z.output<typeof schemas.BindingRanges>
export type BindingRangesInput = z.input<typeof schemas.BindingRanges>
export type CompatibleBindOptions = z.output<
  typeof schemas.CompatibleBindOptions
>
export type CompatibleBindOptionsInput = z.input<
  typeof schemas.CompatibleBindOptions
>
export type Manifest = z.output<typeof schemas.Manifest>
export type ManifestInput = z.input<typeof schemas.Manifest>
export type PackageIndex = z.output<typeof schemas.PackageIndex>
export type PackageIndexInput = z.input<typeof schemas.PackageIndex>
export type BindOptions = z.output<typeof schemas.BindOptions>
export type BindOptionsInput = z.input<typeof schemas.BindOptions>
export type FullIndex = z.output<typeof schemas.FullIndex>
export type FullIndexInput = z.input<typeof schemas.FullIndex>
export type InstalledState = z.output<typeof schemas.InstalledState>
export type InstalledStateInput = z.input<typeof schemas.InstalledState>
export type InstallingInfo = z.output<typeof schemas.InstallingInfo>
export type InstallingInfoInput = z.input<typeof schemas.InstallingInfo>
export type BindInfo = z.output<typeof schemas.BindInfo>
export type BindInfoInput = z.input<typeof schemas.BindInfo>
export type BindParams = z.output<typeof schemas.BindParams>
export type BindParamsInput = z.input<typeof schemas.BindParams>
export type InstallingState = z.output<typeof schemas.InstallingState>
export type InstallingStateInput = z.input<typeof schemas.InstallingState>
export type UpdatingState = z.output<typeof schemas.UpdatingState>
export type UpdatingStateInput = z.input<typeof schemas.UpdatingState>
export type Bindings = z.output<typeof schemas.Bindings>
export type BindingsInput = z.input<typeof schemas.Bindings>
export type PackageState = z.output<typeof schemas.PackageState>
export type PackageStateInput = z.input<typeof schemas.PackageState>
export type Host = z.output<typeof schemas.Host>
export type HostInput = z.input<typeof schemas.Host>
export type Hosts = z.output<typeof schemas.Hosts>
export type HostsInput = z.input<typeof schemas.Hosts>
export type NetworkInfo = z.output<typeof schemas.NetworkInfo>
export type NetworkInfoInput = z.input<typeof schemas.NetworkInfo>
export type PackageDataEntry = z.output<typeof schemas.PackageDataEntry>
export type PackageDataEntryInput = z.input<typeof schemas.PackageDataEntry>
export type ServerInfo = z.output<typeof schemas.ServerInfo>
export type ServerInfoInput = z.input<typeof schemas.ServerInfo>
export type AllPackageData = z.output<typeof schemas.AllPackageData>
export type AllPackageDataInput = z.input<typeof schemas.AllPackageData>
export type Public = z.output<typeof schemas.Public>
export type PublicInput = z.input<typeof schemas.Public>
