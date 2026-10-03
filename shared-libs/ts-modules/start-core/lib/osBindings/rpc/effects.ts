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
          ? Root['_PARAMS'] & RpcParamType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? Root['_PARAMS'] & RpcParamType<Children[Method], ''>
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

export type AcmeProvider = string
export type ActionAccessInput = 'public' | 'dependent' | 'user'
export type ActionId = string
export type ActionIdInput = string
export type ActionInput = {
  eventId: Guid
  spec: Record<string, unknown>
  value: Record<string, unknown> | null
}
export type ActionMetadataInput = {
  name: string
  description: string
  warning?: string | null
  visibility?: ActionVisibilityInput
  allowedStatuses: AllowedStatusesInput
  hasInput: boolean
  group?: string | null
  access?: ActionAccessInput | null
}
export type ActionResult =
  | (
      | ({ version: '0' } & Exclude<ActionResultV0, null>)
      | (null extends ActionResultV0 ? { version: '0' } : never)
    )
  | (
      | ({ version: '1' } & Exclude<ActionResultV1, null>)
      | (null extends ActionResultV1 ? { version: '1' } : never)
    )
export type ActionResultMember = {
  name: string
  description: string | null
} & ActionResultValue
export type ActionResultV0 = {
  message: string
  value: string | null
  copyable: boolean
  qr: boolean
}
export type ActionResultV1 = {
  title: string
  message: string | null
  result: ActionResultValue | null
}
export type ActionResultValue =
  | ({ type: 'single' } & {
      value: string
      copyable: boolean | null
      qr: boolean | null
      masked: boolean | null
      launchable: boolean | null
    })
  | ({ type: 'multiline' } & {
      value: string
      copyable: boolean | null
      qr: boolean | null
      masked: boolean | null
      filename: string | null
    })
  | ({ type: 'group' } & { value: ActionResultMember[] })
export type ActionVisibilityInput = 'hidden' | { disabled: string } | 'enabled'
export type AddSslOptions = {
  preferredExternalPort: number
  addXForwardedHeaders: boolean
  alpn: AlpnInfo | null
  upstreamCertValidation: UpstreamCertValidation | null
  auth: ProxyAuth | null
}
export type AddSslOptionsInput = {
  preferredExternalPort: number
  addXForwardedHeaders?: boolean
  alpn?: CompatibleAlpnInfoInput | null
  upstreamCertValidation?: UpstreamCertValidationInput | null
  auth?: ProxyAuthInput | null
}
export type AddressInfo = {
  username: string | null
  hostId: HostId
  internalPort: number
  scheme: string | null
  sslScheme: string | null
  suffix: string
}
export type AddressInfoInput = {
  username?: string | null
  hostId: HostIdInput
  internalPort: number
  scheme?: string | null
  sslScheme?: string | null
  suffix: string
}
export type AlgorithmInput = 'ecdsa' | 'ed25519'
export type AllowedStatusesInput = 'only-running' | 'only-stopped' | 'any'
export type AlpnInfo = MaybeUtf8String[]
export type Api = {
  _PARAMS: {}
  _CHILDREN: {
    action: {
      _PARAMS: {}
      _CHILDREN: {
        clear: { _PARAMS: ClearActionsParamsInput; _RETURN: null }
        'clear-tasks': { _PARAMS: ClearTasksParamsInput; _RETURN: null }
        'create-task': { _PARAMS: CreateTaskParamsInput; _RETURN: null }
        export: { _PARAMS: ExportActionParamsInput; _RETURN: null }
        'get-input': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            GetActionInputParamsInput
          _RETURN: ActionInput | null
        }
        run: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            EffectsRunActionParamsInput
          _RETURN: ActionResult | null
        }
      }
    }
    bind: { _PARAMS: BindParamsInput; _RETURN: null }
    'bind-range': { _PARAMS: BindRangeParamsInput; _RETURN: null }
    'check-dependencies': {
      _PARAMS: CheckDependenciesParamInput
      _RETURN: CheckDependenciesResult[]
    }
    'clear-bindings': { _PARAMS: ClearBindingsParamsInput; _RETURN: null }
    'clear-callbacks': { _PARAMS: ClearCallbacksParamsInput; _RETURN: null }
    'clear-service-interfaces': {
      _PARAMS: ClearServiceInterfacesParamsInput
      _RETURN: null
    }
    echo: { _PARAMS: EchoParamsInput; _RETURN: string }
    'export-range-service-interface': {
      _PARAMS: ExportRangeServiceInterfaceParamsInput
      _RETURN: null
    }
    'export-service-interface': {
      _PARAMS: ExportServiceInterfaceParamsInput
      _RETURN: null
    }
    'get-container-ip': {
      _PARAMS: GetContainerIpParamsInput
      _RETURN: string | null
    }
    'get-data-version': { _PARAMS: {}; _RETURN: string | null }
    'get-dependencies': { _PARAMS: {}; _RETURN: DependencyRequirement[] }
    'get-host-info': { _PARAMS: GetHostInfoParamsInput; _RETURN: Host | null }
    'get-installed-packages': { _PARAMS: {}; _RETURN: PackageId[] }
    'get-os-ip': { _PARAMS: {}; _RETURN: string }
    'get-outbound-gateway': {
      _PARAMS: GetOutboundGatewayParamsInput
      _RETURN: GatewayId
    }
    'get-service-interface': {
      _PARAMS: GetServiceInterfaceParamsInput
      _RETURN: ServiceInterface | null
    }
    'get-service-manifest': {
      _PARAMS: GetServiceManifestParamsInput
      _RETURN: Manifest
    }
    'get-service-port-forward': {
      _PARAMS: GetServicePortForwardParamsInput
      _RETURN: NetInfo | null
    }
    'get-ssl-certificate': {
      _PARAMS: GetSslCertificateParamsInput
      _RETURN: string[]
    }
    'get-ssl-key': { _PARAMS: GetSslKeyParamsInput; _RETURN: string }
    'get-status': { _PARAMS: GetStatusParamsInput; _RETURN: StatusInfo | null }
    'get-system-smtp': {
      _PARAMS: GetSystemSmtpParamsInput
      _RETURN: SmtpValue | null
    }
    'git-info': { _PARAMS: {}; _RETURN: string }
    'list-service-interfaces': {
      _PARAMS: ListServiceInterfacesParamsInput
      _RETURN: { [key: string]: ServiceInterface }
    }
    mount: { _PARAMS: MountParamsInput; _RETURN: null }
    notification: {
      _PARAMS: {}
      _CHILDREN: {
        create: { _PARAMS: CreateNotificationParamsInput; _RETURN: null }
      }
    }
    plugin: {
      _PARAMS: {}
      _CHILDREN: {
        url: {
          _PARAMS: {}
          _CHILDREN: {
            'clear-urls': {
              _PARAMS: UrlPluginClearUrlsParamsInput
              _RETURN: null
            }
            'export-url': {
              _PARAMS: UrlPluginExportUrlParamsInput
              _RETURN: null
            }
            register: { _PARAMS: UrlPluginRegisterParamsInput; _RETURN: null }
          }
        }
      }
    }
    rebuild: { _PARAMS: {}; _RETURN: null }
    restart: { _PARAMS: {}; _RETURN: null }
    'retire-binding': { _PARAMS: RetireBindingParamsInput; _RETURN: boolean }
    'retire-host': { _PARAMS: RetireHostParamsInput; _RETURN: boolean }
    'set-backup-progress': { _PARAMS: SetBackupProgressInput; _RETURN: null }
    'set-data-version': { _PARAMS: SetDataVersionParamsInput; _RETURN: null }
    'set-dependencies': { _PARAMS: SetDependenciesParamsInput; _RETURN: null }
    'set-health': { _PARAMS: SetHealthInput; _RETURN: null }
    'set-init-progress': { _PARAMS: SetInitProgressInput; _RETURN: null }
    'set-main-status': { _PARAMS: SetMainStatusInput; _RETURN: null }
    shutdown: { _PARAMS: {}; _RETURN: null }
    subcontainer: {
      _PARAMS: {}
      _CHILDREN: {
        'create-fs': {
          _PARAMS: CreateSubcontainerFsParamsInput
          _RETURN: [string, Guid]
        }
        'destroy-fs': {
          _PARAMS: DestroySubcontainerFsParamsInput
          _RETURN: null
        }
      }
    }
  }
}
export type BasicCredential = { username: string; password: string }
export type BasicCredentialInput = { username: string; password: string }
export type BindIdInput = { id: HostIdInput; internalPort: number }
export type BindInfo = {
  enabled: boolean
  options: BindOptions
  net: NetInfo
  addresses: DerivedAddressInfo
  interfaces: { [key: string]: ServiceInterface }
}
export type BindOptions = {
  preferredExternalPort: number
  addSsl: AddSslOptions | null
  secure: Security | null
}
export type BindOptionsInput = {
  preferredExternalPort: number
  addSsl?: AddSslOptionsInput | null
  secure?: SecurityInput | null
}
export type BindParamsInput = {
  id: HostIdInput
  internalPort: number
} & BindOptionsInput
export type BindRangeParamsInput = {
  id: HostIdInput
  internalStartPort: number
  externalStartPort: number
  numberOfPorts: number
}
export type BindingRanges = { [key: string]: RangeBindInfo }
export type Bindings = { [key: string]: BindInfo }
export type BuildArg = string | { env: string }
export type CallbackIdInput = number
export type CheckDependenciesParamInput = {
  packageIds?: PackageIdInput[] | null
}
export type CheckDependenciesResult = {
  packageId: PackageId
  title: string | null
  installedVersion: Version | null
  satisfies: Version[]
  isRunning: boolean
  tasks: { [key: string]: TaskEntry }
  healthChecks: { [key: string]: NamedHealthCheckResult }
}
export type ClearActionsParamsInput = { except: ActionIdInput[] }
export type ClearBindingsParamsInput = { except?: BindIdInput[] }
export type ClearCallbacksParamsInput =
  | { only: number[] }
  | { except: number[] }
export type ClearServiceInterfacesParamsInput = {
  except: ServiceInterfaceIdInput[]
}
export type ClearTasksParamsInput = { only: string[] } | { except: string[] }
export type CompatibleAlpnInfoInput =
  | MaybeUtf8StringInput[]
  | LegacyAlpnInfoInput
export type CreateNotificationParamsInput = {
  level: NotificationLevelInput
  title: string
  message: string
  data?: string | null
}
export type CreateSubcontainerFsParamsInput = {
  imageId: ImageIdInput
  name?: string | null
}
export type CreateTaskParamsInput = { replayId: ReplayIdInput } & TaskParams
export type CurrentDependencyKind =
  | { kind: 'exists' }
  | ({ kind: 'running' } & { healthChecks: string[] })
export type DepInfo = {
  description: LocaleString | null
  optional: boolean
  versionRange: string | null
} & Partial<Exclude<CurrentDependencyKind | null, null>> &
  Partial<Exclude<MetadataSrc | null, null>>
export type Dependencies = { [key: string]: DepInfo }
export type DependencyRequirement =
  | ({ kind: 'running' } & {
      id: PackageId
      healthChecks: HealthCheckId[]
      versionRange: string
    })
  | ({ kind: 'exists' } & { id: PackageId; versionRange: string })
export type DependencyRequirementInput =
  | ({ kind: 'running' } & {
      id: PackageIdInput
      healthChecks: HealthCheckIdInput[]
      versionRange: string
    })
  | ({ kind: 'exists' } & { id: PackageIdInput; versionRange: string })
export type DerivedAddressInfo = {
  enabled: string[]
  disabled: [string, number][]
  guaWan: string[]
  lanEnabled: [string, number][]
  available: HostnameInfo[]
}
export type Description = { short: LocaleString; long: LocaleString }
export type DesiredStatus =
  | { main: 'stopped' }
  | ({ main: 'restarting' } & { restartAgain: boolean })
  | { main: 'running' }
  | ({ main: 'backing-up' } & { onComplete: StartStop })
  | ({ main: 'updating' } & { onComplete: StartStop })
export type DestroySubcontainerFsParamsInput = { guid: GuidInput }
export type DeviceFilter = {
  description: string
  class: 'processor' | 'display'
  product: string | null
  vendor: string | null
  capabilities: string[] | null
  driver: string | null
}
export type EchoParamsInput = { message: string }
export type EffectsRunActionParamsInput = {
  packageId?: PackageIdInput | null
  actionId: ActionIdInput
  input: unknown
}
export type ErrorData = { details: string; debug: string; info: unknown }
export type ExportActionParamsInput = {
  id: ActionIdInput
  metadata: ActionMetadataInput
}
export type ExportRangeServiceInterfaceParamsInput = {
  hostId: HostIdInput
  internalStartPort: number
  id: ServiceInterfaceIdInput
  name: string
  description: string
  scheme?: string | null
}
export type ExportServiceInterfaceParamsInput = {
  id: ServiceInterfaceIdInput
  name: string
  description: string
  masked: boolean
  addressInfo: AddressInfoInput
  type: ServiceInterfaceTypeInput
  preferredLauncherAddress?: string | null
}
export type FullProgressInput = {
  overall: ProgressInput
  phases: NamedProgressInput[]
}
export type GatewayId = string
export type GetActionInputParamsInput = {
  packageId?: PackageIdInput | null
  actionId: ActionIdInput
  prefill?: Record<string, unknown> | null
}
export type GetContainerIpParamsInput = {
  packageId?: PackageIdInput | null
  callback?: CallbackIdInput | null
}
export type GetHostInfoParamsInput = {
  hostId: HostIdInput
  packageId?: PackageIdInput | null
  callback?: CallbackIdInput | null
}
export type GetOutboundGatewayParamsInput = {
  callback?: CallbackIdInput | null
}
export type GetServiceInterfaceParamsInput = {
  packageId?: PackageIdInput | null
  serviceInterfaceId: ServiceInterfaceIdInput
  callback?: CallbackIdInput | null
}
export type GetServiceManifestParamsInput = {
  packageId: PackageIdInput
  callback?: CallbackIdInput | null
}
export type GetServicePortForwardParamsInput = {
  packageId?: PackageIdInput | null
  hostId: HostIdInput
  internalPort: number
}
export type GetSslCertificateParamsInput = {
  hostnames: string[]
  algorithm?: AlgorithmInput | null
  callback?: CallbackIdInput | null
}
export type GetSslKeyParamsInput = {
  hostnames: string[]
  algorithm?: AlgorithmInput | null
}
export type GetStatusParamsInput = {
  packageId?: PackageIdInput | null
  callback?: CallbackIdInput | null
}
export type GetSystemSmtpParamsInput = { callback?: CallbackIdInput | null }
export type GitHash = string
export type Guid = string
export type GuidInput = string
export type HardwareRequirements = {
  device: DeviceFilter[]
  ram: number | null
  arch: string[] | null
}
export type HealthCheckId = string
export type HealthCheckIdInput = string
export type Host = {
  bindings: Bindings
  bindingRanges: BindingRanges
  publicDomains: { [key: string]: PublicDomainConfig }
  privateDomains: { [key: string]: GatewayId[] }
  portForwards: PortForward[]
}
export type HostId = string
export type HostIdInput = string
export type HostnameInfo = {
  ssl: boolean
  public: boolean
  hostname: string
  port: number | null
  metadata: HostnameMetadata
}
export type HostnameMetadata =
  | ({ kind: 'ipv4' } & { gateway: GatewayId })
  | ({ kind: 'ipv6' } & { gateway: GatewayId; scopeId: number })
  | ({ kind: 'mdns' } & { gateways: GatewayId[] })
  | ({ kind: 'private-domain' } & { gateways: GatewayId[] })
  | ({ kind: 'public-domain' } & { gateway: GatewayId })
  | ({ kind: 'plugin' } & {
      packageId: PackageId
      removeAction: ActionId | null
      overflowActions: ActionId[]
      info: unknown
    })
export type IdMapInput = { fromId: number; toId: number; range: number }
export type ImageConfig = {
  source: ImageSource
  arch: string[]
  emulateMissing: boolean
  nvidiaContainer: boolean
}
export type ImageIdInput = string
export type ImageSource =
  | 'packed'
  | {
      dockerBuild: {
        workdir: string | null
        dockerfile: string | null
        buildArgs?: { [key: string]: BuildArg } | null
      }
    }
  | { dockerTag: string }
export type IoFormatInput =
  | 'json'
  | 'json-pretty'
  | 'yaml'
  | 'cbor'
  | 'toml'
  | 'toml-pretty'
export type LegacyAlpnInfoInput =
  | 'reflect'
  | { specified: MaybeUtf8StringInput[] }
export type ListServiceInterfacesParamsInput = {
  packageId?: PackageIdInput | null
  callback?: CallbackIdInput | null
}
export type LocaleString = string | Record<string, string>
export type Manifest = {
  id: PackageId
  version: Version
  canMigrateTo: string
  canMigrateFrom: string
  images: { [key: string]: ImageConfig }
  volumes: VolumeId[]
  dependencies: Dependencies
  hardwareRequirements: HardwareRequirements
} & PackageMetadata
export type MaybeUtf8String = string | number[]
export type MaybeUtf8StringInput = string | number[]
export type Metadata = { title: LocaleString; icon: PathOrUrl }
export type MetadataSrc = { metadata: Metadata } | { s9pk: PathOrUrl | null }
export type MountParamsInput = { location: string; target: MountTargetInput }
export type MountTargetInput = {
  packageId: PackageIdInput
  volumeId: VolumeIdInput
  subpath?: string | null
  readonly: boolean
  idmap?: IdMapInput[]
}
export type NamedHealthCheckResult = {
  name: string
} & NamedHealthCheckResultKind
export type NamedHealthCheckResultInput = {
  name: string
} & NamedHealthCheckResultKindInput
export type NamedHealthCheckResultKind =
  | ({ result: 'success' } & { message: string | null })
  | ({ result: 'disabled' } & { message: string | null })
  | ({ result: 'starting' } & { message: string | null })
  | ({ result: 'waiting' } & { message: string | null })
  | ({ result: 'loading' } & { message: string })
  | ({ result: 'failure' } & { message: string })
export type NamedHealthCheckResultKindInput =
  | ({ result: 'success' } & { message?: string | null })
  | ({ result: 'disabled' } & { message?: string | null })
  | ({ result: 'starting' } & { message?: string | null })
  | ({ result: 'waiting' } & { message?: string | null })
  | ({ result: 'loading' } & { message: string })
  | ({ result: 'failure' } & { message: string })
export type NamedProgressInput = { name: string; progress: ProgressInput }
export type NetInfo = {
  assignedPort: number | null
  assignedSslPort: number | null
}
export type NotificationLevelInput = 'success' | 'info' | 'warning' | 'error'
export type PackageId = string
export type PackageIdInput = string
export type PackageMetadata = {
  title: string
  description: Description
  releaseNotes: LocaleString
  preDownloadAlert?: PreDownloadAlert | null
  gitHash: GitHash | null
  license: string
  packageRepo: string
  upstreamRepo: string
  marketingUrl: string | null
  donationUrl: string | null
  osVersion: string
  sdkVersion: string | null
  hardwareAcceleration: boolean
  userspaceFilesystems: boolean
  virtualNetworking: boolean
  hardwareVirtualization: boolean
  plugins: PluginId[]
  satisfies: Version[]
}
export type PathOrUrl = string
export type PluginHostnameInfoInput = {
  packageId?: PackageIdInput | null
  hostId: HostIdInput
  internalPort: number
  ssl: boolean
  public: boolean
  hostname: string
  port?: number | null
  info?: unknown
}
export type PluginId = 'url-v0'
export type PortForward = {
  src: string
  dst: string
  gateway: GatewayId
  count: number
  local: boolean
}
export type PreDownloadAlert = {
  message: LocaleString
  when: PreDownloadAlertWhen
}
export type PreDownloadAlertWhen = { sourceVersion: string }
export type ProgressInput =
  | null
  | boolean
  | { done: number; total?: number | null; units?: ProgressUnitsInput | null }
  | FullProgressInput
export type ProgressUnitsInput = 'bytes' | 'steps'
export type ProxyAuth =
  | ({ type: 'bearer' } & { tokens: string[]; realm: string | null })
  | ({ type: 'basic' } & {
      credentials: BasicCredential[]
      realm: string | null
    })
export type ProxyAuthInput =
  | ({ type: 'bearer' } & { tokens: string[]; realm?: string | null })
  | ({ type: 'basic' } & {
      credentials: BasicCredentialInput[]
      realm?: string | null
    })
export type PublicDomainConfig = {
  gateway: GatewayId
  acme: AcmeProvider | null
}
export type RangeBindInfo = {
  enabled: boolean
  externalStartPort: number
  numberOfPorts: number
  addresses: DerivedAddressInfo
  interface: RangeServiceInterface | null
}
export type RangeServiceInterface = {
  id: ServiceInterfaceId
  name: string
  description: string
  scheme: string | null
}
export type ReplayIdInput = string
export type RetireBindingParamsInput = { id: HostIdInput; internalPort: number }
export type RetireHostParamsInput = { id: HostIdInput }
export type Security = { ssl: boolean }
export type SecurityInput = { ssl: boolean }
export type ServiceInterface = {
  id: ServiceInterfaceId
  name: string
  description: string
  masked: boolean
  addressInfo: AddressInfo
  type: ServiceInterfaceType
  preferredLauncherAddress: string | null
}
export type ServiceInterfaceId = string
export type ServiceInterfaceIdInput = string
export type ServiceInterfaceType = 'ui' | 'p2p' | 'api'
export type ServiceInterfaceTypeInput = 'ui' | 'p2p' | 'api'
export type SetBackupProgressInput = { progress: ProgressInput }
export type SetDataVersionParamsInput = { version?: string | null }
export type SetDependenciesParamsInput = {
  dependencies: DependencyRequirementInput[]
}
export type SetHealthInput = {
  id: HealthCheckIdInput
} & NamedHealthCheckResultInput
export type SetInitProgressInput = { progress: ProgressInput }
export type SetMainStatusInput = { status: SetMainStatusStatusInput }
export type SetMainStatusStatusInput = 'running' | 'stopped'
export type SmtpSecurity = 'starttls' | 'tls'
export type SmtpValue = {
  host: string
  port: number
  from: string
  username: string
  password: string | null
  security: SmtpSecurity
}
export type StartStop = 'start' | 'stop'
export type StatusInfo = {
  health: { [key: string]: NamedHealthCheckResult }
  error: ErrorData | null
  started: string | null
  desired: DesiredStatus
}
export type Task = {
  packageId: PackageId
  actionId: ActionId
  severity: TaskSeverity
  reason: string | null
  when: TaskTrigger | null
  input: TaskInput | null
}
export type TaskCondition = 'input-not-matches'
export type TaskConditionInput = 'input-not-matches'
export type TaskEntry = { task: Task; active: boolean }
export type TaskInput = { kind: 'partial' } & {
  accept: Record<string, unknown>[]
  set: Record<string, unknown>
}
export type TaskInputInput = TaskInputReprInput
export type TaskInputReprInput = { kind: 'partial' } & {
  accept?: unknown[] | null
  set?: unknown | null
  value?: unknown | null
}
export type TaskParams = {
  packageId: PackageIdInput
  actionId: ActionIdInput
  severity?: TaskSeverityInput
  reason?: string | null
  when?: TaskTriggerInput | null
  input?: TaskInputInput | null
}
export type TaskSeverity = 'optional' | 'important' | 'critical'
export type TaskSeverityInput = 'optional' | 'important' | 'critical'
export type TaskTrigger = { once: boolean; condition: TaskCondition }
export type TaskTriggerInput = { once?: boolean; condition: TaskConditionInput }
export type UpstreamCertValidation = 'disable' | { certificate: string }
export type UpstreamCertValidationInput = 'disable' | { certificate: string }
export type UrlPluginClearUrlsParamsInput = {
  except: PluginHostnameInfoInput[]
}
export type UrlPluginExportUrlParamsInput = {
  hostnameInfo: PluginHostnameInfoInput
  removeAction?: ActionIdInput | null
  overflowActions: ActionIdInput[]
}
export type UrlPluginRegisterParamsInput = { tableAction: ActionIdInput }
export type Version = string
export type VolumeId = string
export type VolumeIdInput = string
