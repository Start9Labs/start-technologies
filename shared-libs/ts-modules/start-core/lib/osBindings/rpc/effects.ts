import type { z } from 'zod'
import type * as schemas from './effects.schemas'
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
export type ActionAccessInput = z.input<typeof schemas.ActionAccess>
export type ActionId = z.output<typeof schemas.ActionId>
export type ActionIdInput = z.input<typeof schemas.ActionId>
export type ActionResultV0 = z.output<typeof schemas.ActionResultV0>
export type ActionResultValue = z.output<typeof schemas.ActionResultValue>
export type ActionVisibilityInput = z.input<typeof schemas.ActionVisibility>
export type AlgorithmInput = z.input<typeof schemas.Algorithm>
export type AllowedStatusesInput = z.input<typeof schemas.AllowedStatuses>
export type BasicCredential = z.output<typeof schemas.BasicCredential>
export type BasicCredentialInput = z.input<typeof schemas.BasicCredential>
export type BuildArg = z.output<typeof schemas.BuildArg>
export type CallbackIdInput = z.input<typeof schemas.CallbackId>
export type ClearCallbacksParamsInput = z.input<
  typeof schemas.ClearCallbacksParams
>
export type ClearTasksParamsInput = z.input<typeof schemas.ClearTasksParams>
export type DeviceFilter = z.output<typeof schemas.DeviceFilter>
export type EchoParamsInput = z.input<typeof schemas.EchoParams>
export type ErrorData = z.output<typeof schemas.ErrorData>
export type GatewayId = z.output<typeof schemas.GatewayId>
export type GitHash = z.output<typeof schemas.GitHash>
export type Guid = z.output<typeof schemas.Guid>
export type GuidInput = z.input<typeof schemas.Guid>
export type HealthCheckId = z.output<typeof schemas.HealthCheckId>
export type HealthCheckIdInput = z.input<typeof schemas.HealthCheckId>
export type HostId = z.output<typeof schemas.HostId>
export type HostIdInput = z.input<typeof schemas.HostId>
export type IdMapInput = z.input<typeof schemas.IdMap>
export type ImageIdInput = z.input<typeof schemas.ImageId>
export type IoFormatInput = z.input<typeof schemas.IoFormat>
export type LocaleString = z.output<typeof schemas.LocaleString>
export type MaybeUtf8String = z.output<typeof schemas.MaybeUtf8String>
export type MaybeUtf8StringInput = z.input<typeof schemas.MaybeUtf8String>
export type NamedHealthCheckResultKind = z.output<
  typeof schemas.NamedHealthCheckResultKind
>
export type NamedHealthCheckResultKindInput = z.input<
  typeof schemas.NamedHealthCheckResultKind
>
export type NamedProgressInput = z.input<typeof schemas.NamedProgress>
export type NetInfo = z.output<typeof schemas.NetInfo>
export type NotificationLevelInput = z.input<typeof schemas.NotificationLevel>
export type PackageId = z.output<typeof schemas.PackageId>
export type PackageIdInput = z.input<typeof schemas.PackageId>
export type PathOrUrl = z.output<typeof schemas.PathOrUrl>
export type PluginId = z.output<typeof schemas.PluginId>
export type PreDownloadAlertWhen = z.output<typeof schemas.PreDownloadAlertWhen>
export type ProgressUnitsInput = z.input<typeof schemas.ProgressUnits>
export type ReplayIdInput = z.input<typeof schemas.ReplayId>
export type Security = z.output<typeof schemas.Security>
export type SecurityInput = z.input<typeof schemas.Security>
export type ServiceInterfaceId = z.output<typeof schemas.ServiceInterfaceId>
export type ServiceInterfaceIdInput = z.input<typeof schemas.ServiceInterfaceId>
export type ServiceInterfaceType = z.output<typeof schemas.ServiceInterfaceType>
export type ServiceInterfaceTypeInput = z.input<
  typeof schemas.ServiceInterfaceType
>
export type SetDataVersionParamsInput = z.input<
  typeof schemas.SetDataVersionParams
>
export type SetMainStatusStatusInput = z.input<
  typeof schemas.SetMainStatusStatus
>
export type SmtpSecurity = z.output<typeof schemas.SmtpSecurity>
export type StartStop = z.output<typeof schemas.StartStop>
export type TaskCondition = z.output<typeof schemas.TaskCondition>
export type TaskConditionInput = z.input<typeof schemas.TaskCondition>
export type TaskInputRepr = z.output<typeof schemas.TaskInputRepr>
export type TaskInputReprInput = z.input<typeof schemas.TaskInputRepr>
export type TaskSeverity = z.output<typeof schemas.TaskSeverity>
export type TaskSeverityInput = z.input<typeof schemas.TaskSeverity>

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
export type Version = z.output<typeof schemas.Version>
export type VolumeId = z.output<typeof schemas.VolumeId>
export type VolumeIdInput = z.input<typeof schemas.VolumeId>
export type ActionInput = z.output<typeof schemas.ActionInput>
export type ActionMetadataInput = z.input<typeof schemas.ActionMetadata>
export type ActionResultMember = z.output<typeof schemas.ActionResultMember>
export type ActionResultV1 = z.output<typeof schemas.ActionResultV1>
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
export type BindIdInput = z.input<typeof schemas.BindId>
export type BindRangeParamsInput = z.input<typeof schemas.BindRangeParams>
export type CheckDependenciesParamInput = z.input<
  typeof schemas.CheckDependenciesParam
>
export type ClearActionsParamsInput = z.input<typeof schemas.ClearActionsParams>
export type ClearServiceInterfacesParamsInput = z.input<
  typeof schemas.ClearServiceInterfacesParams
>
export type CreateNotificationParamsInput = z.input<
  typeof schemas.CreateNotificationParams
>
export type CreateSubcontainerFsParamsInput = z.input<
  typeof schemas.CreateSubcontainerFsParams
>
export type CurrentDependencyKind = z.output<
  typeof schemas.CurrentDependencyKind
>
export type DependencyRequirement = z.output<
  typeof schemas.DependencyRequirement
>
export type DependencyRequirementInput = z.input<
  typeof schemas.DependencyRequirement
>
export type Description = z.output<typeof schemas.Description>
export type DesiredStatus = z.output<typeof schemas.DesiredStatus>
export type DestroySubcontainerFsParamsInput = z.input<
  typeof schemas.DestroySubcontainerFsParams
>
export type EffectsRunActionParamsInput = z.input<
  typeof schemas.EffectsRunActionParams
>
export type ExportRangeServiceInterfaceParamsInput = z.input<
  typeof schemas.ExportRangeServiceInterfaceParams
>
export type FullProgressInput = z.input<typeof schemas.FullProgress>
export type GetActionInputParamsInput = z.input<
  typeof schemas.GetActionInputParams
>
export type GetContainerIpParamsInput = z.input<
  typeof schemas.GetContainerIpParams
>
export type GetHostInfoParamsInput = z.input<typeof schemas.GetHostInfoParams>
export type GetOutboundGatewayParamsInput = z.input<
  typeof schemas.GetOutboundGatewayParams
>
export type GetServiceInterfaceParamsInput = z.input<
  typeof schemas.GetServiceInterfaceParams
>
export type GetServiceManifestParamsInput = z.input<
  typeof schemas.GetServiceManifestParams
>
export type GetServicePortForwardParamsInput = z.input<
  typeof schemas.GetServicePortForwardParams
>
export type GetSslCertificateParamsInput = z.input<
  typeof schemas.GetSslCertificateParams
>
export type GetSslKeyParamsInput = z.input<typeof schemas.GetSslKeyParams>
export type GetStatusParamsInput = z.input<typeof schemas.GetStatusParams>
export type GetSystemSmtpParamsInput = z.input<
  typeof schemas.GetSystemSmtpParams
>
export type HardwareRequirements = z.output<typeof schemas.HardwareRequirements>
export type HostnameMetadata = z.output<typeof schemas.HostnameMetadata>
export type ImageSource = z.output<typeof schemas.ImageSource>
export type LegacyAlpnInfo = z.output<typeof schemas.LegacyAlpnInfo>
export type LegacyAlpnInfoInput = z.input<typeof schemas.LegacyAlpnInfo>
export type ListServiceInterfacesParamsInput = z.input<
  typeof schemas.ListServiceInterfacesParams
>
export type Metadata = z.output<typeof schemas.Metadata>
export type MountTargetInput = z.input<typeof schemas.MountTarget>
export type NamedHealthCheckResult = z.output<
  typeof schemas.NamedHealthCheckResult
>
export type NamedHealthCheckResultInput = z.input<
  typeof schemas.NamedHealthCheckResult
>
export type PluginHostnameInfoInput = z.input<typeof schemas.PluginHostnameInfo>
export type PortForward = z.output<typeof schemas.PortForward>
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
export type RetireBindingParamsInput = z.input<
  typeof schemas.RetireBindingParams
>
export type RetireHostParamsInput = z.input<typeof schemas.RetireHostParams>
export type SetMainStatusInput = z.input<typeof schemas.SetMainStatus>
export type SmtpValue = z.output<typeof schemas.SmtpValue>
export type TaskInput = z.output<typeof schemas.TaskInput>
export type TaskInputInput = z.input<typeof schemas.TaskInput>
export type TaskTrigger = z.output<typeof schemas.TaskTrigger>
export type TaskTriggerInput = z.input<typeof schemas.TaskTrigger>
export type UrlPluginRegisterParamsInput = z.input<
  typeof schemas.UrlPluginRegisterParams
>
export type ActionResult = z.output<typeof schemas.ActionResult>
export type ClearBindingsParamsInput = z.input<
  typeof schemas.ClearBindingsParams
>
export type CompatibleAlpnInfo = z.output<typeof schemas.CompatibleAlpnInfo>
export type CompatibleAlpnInfoInput = z.input<typeof schemas.CompatibleAlpnInfo>
export type ExportActionParamsInput = z.input<typeof schemas.ExportActionParams>
export type ExportServiceInterfaceParamsInput = z.input<
  typeof schemas.ExportServiceInterfaceParams
>
export type HostnameInfo = z.output<typeof schemas.HostnameInfo>
export type ImageConfigRepr = z.output<typeof schemas.ImageConfigRepr>
export type MetadataSrc = z.output<typeof schemas.MetadataSrc>
export type MountParamsInput = z.input<typeof schemas.MountParams>
export type PackageMetadata = z.output<typeof schemas.PackageMetadata>
export type ProgressInput = z.input<typeof schemas.Progress>
export type ProxyOptions = z.output<typeof schemas.ProxyOptions>
export type ProxyOptionsInput = z.input<typeof schemas.ProxyOptions>
export type ServiceInterface = z.output<typeof schemas.ServiceInterface>
export type SetDependenciesParamsInput = z.input<
  typeof schemas.SetDependenciesParams
>
export type SetHealthInput = z.input<typeof schemas.SetHealth>
export type StatusInfo = z.output<typeof schemas.StatusInfo>
export type Task = z.output<typeof schemas.Task>
export type TaskParams = z.input<typeof schemas.Task>
export type UrlPluginClearUrlsParamsInput = z.input<
  typeof schemas.UrlPluginClearUrlsParams
>
export type UrlPluginExportUrlParamsInput = z.input<
  typeof schemas.UrlPluginExportUrlParams
>
export type AddSslOptions = z.output<typeof schemas.AddSslOptions>
export type AddSslOptionsInput = z.input<typeof schemas.AddSslOptions>
export type CreateTaskParamsInput = z.input<typeof schemas.CreateTaskParams>
export type DepInfo = z.output<typeof schemas.DepInfo>
export type DerivedAddressInfo = z.output<typeof schemas.DerivedAddressInfo>
export type ImageConfig = z.output<typeof schemas.ImageConfig>
export type SetBackupProgressInput = z.input<typeof schemas.SetBackupProgress>
export type SetInitProgressInput = z.input<typeof schemas.SetInitProgress>
export type TaskEntry = z.output<typeof schemas.TaskEntry>
export type CheckDependenciesResult = z.output<
  typeof schemas.CheckDependenciesResult
>
export type Dependencies = z.output<typeof schemas.Dependencies>
export type LegacyProxySslOptions = z.output<
  typeof schemas.LegacyProxySslOptions
>
export type LegacyProxySslOptionsInput = z.input<
  typeof schemas.LegacyProxySslOptions
>

/**
 * Contiguous port-range binding (e.g. WebRTC/STUN/TURN RTP ranges).
 *
 * Keyed by `internal_start_port` in [`BindingRanges`]. The range covers
 * `internal_start_port..(internal_start_port + number_of_ports)` and is
 * forwarded through a single iptables rule per protocol per gateway,
 * preserving the destination port number.
 */
export type RangeBindInfo = z.output<typeof schemas.RangeBindInfo>
export type BindingRanges = z.output<typeof schemas.BindingRanges>
export type CompatibleBindOptions = z.output<
  typeof schemas.CompatibleBindOptions
>
export type CompatibleBindOptionsInput = z.input<
  typeof schemas.CompatibleBindOptions
>
export type Manifest = z.output<typeof schemas.Manifest>
export type BindOptions = z.output<typeof schemas.BindOptions>
export type BindOptionsInput = z.input<typeof schemas.BindOptions>
export type BindInfo = z.output<typeof schemas.BindInfo>
export type BindParamsInput = z.input<typeof schemas.BindParams>
export type Bindings = z.output<typeof schemas.Bindings>
export type Host = z.output<typeof schemas.Host>
export type Api = RpcSchemaTree<typeof schemas.Api>
