export type AcceptSigners =
  | { signer: AnyVerifyingKey }
  | { any: AcceptSigners[] }
  | { all: AcceptSigners[] }
export type AcceptSignersInput =
  | { signer: AnyVerifyingKeyInput }
  | { any: AcceptSignersInput[] }
  | { all: AcceptSignersInput[] }
export type AcmeProvider = string
export type AcmeProviderInput = string
export type AcmeSettings = { contact: string[] }
export type AcmeSettingsInput = { contact: string[] }
export type ActionAccess = 'public' | 'dependent' | 'user'
export type ActionAccessInput = 'public' | 'dependent' | 'user'
export type ActionId = string
export type ActionIdInput = string
export type ActionInput = {
  eventId: Guid
  spec: Record<string, unknown>
  value: Record<string, unknown> | null
}
export type ActionInputInput = {
  eventId?: GuidInput
  spec: Record<string, unknown>
  value?: Record<string, unknown> | null
}
export type ActionMetadata = {
  /**
   * A human-readable name
   */
  name: string
  /**
   * A detailed description of what the action will do
   */
  description: string
  /**
   * Presents as an alert prior to executing the action. Should be used sparingly but important if the action could have harmful, unintended consequences
   */
  warning: string | null
  /**
   * One of: "enabled", "hidden", or { disabled: "" }
   *   - "enabled" - the action is available be run
   *   - "hidden" - the action cannot be seen or run
   *   - { disabled: "example explanation" } means the action is visible but cannot be run. Replace "example explanation" with a reason why the action is disable to prevent user confusion.
   */
  visibility: ActionVisibility
  /**
   * One of: "only-stopped", "only-running", "all"
   *   - "only-stopped" - the action can only be run when the service is stopped
   *   - "only-running" - the action can only be run when the service is running
   *   - "any" - the action can only be run regardless of the service's status
   */
  allowedStatuses: AllowedStatuses
  hasInput: boolean
  /**
   * If provided, this action will be nested under a header of this value, along with other actions of the same group
   */
  group: string | null
  /**
   * Who is allowed to invoke this action directly via `effects.action.run`.
   *   - "public" — any installed package
   *   - "dependent" — only services that declare this package as a current dependency
   *   - "user" — only the user (other services must create a task). Default when omitted.
   * Services that lack direct access can always queue a task with `effects.action.createTask`.
   */
  access: ActionAccess | null
}
export type ActionMetadataInput = {
  /**
   * A human-readable name
   */
  name: string
  /**
   * A detailed description of what the action will do
   */
  description: string
  /**
   * Presents as an alert prior to executing the action. Should be used sparingly but important if the action could have harmful, unintended consequences
   */
  warning?: string | null
  /**
   * One of: "enabled", "hidden", or { disabled: "" }
   *   - "enabled" - the action is available be run
   *   - "hidden" - the action cannot be seen or run
   *   - { disabled: "example explanation" } means the action is visible but cannot be run. Replace "example explanation" with a reason why the action is disable to prevent user confusion.
   */
  visibility?: ActionVisibilityInput
  /**
   * One of: "only-stopped", "only-running", "all"
   *   - "only-stopped" - the action can only be run when the service is stopped
   *   - "only-running" - the action can only be run when the service is running
   *   - "any" - the action can only be run regardless of the service's status
   */
  allowedStatuses: AllowedStatusesInput
  hasInput: boolean
  /**
   * If provided, this action will be nested under a header of this value, along with other actions of the same group
   */
  group?: string | null
  /**
   * Who is allowed to invoke this action directly via `effects.action.run`.
   *   - "public" — any installed package
   *   - "dependent" — only services that declare this package as a current dependency
   *   - "user" — only the user (other services must create a task). Default when omitted.
   * Services that lack direct access can always queue a task with `effects.action.createTask`.
   */
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
export type ActionResultInput =
  | (
      | ({ version: '0' } & Exclude<ActionResultV0Input, null>)
      | (null extends ActionResultV0Input ? { version: '0' } : never)
    )
  | (
      | ({ version: '1' } & Exclude<ActionResultV1Input, null>)
      | (null extends ActionResultV1Input ? { version: '1' } : never)
    )
export type ActionResultMember = {
  /**
   * A human-readable name or title of the value, such as "Last Active" or "Login Password"
   */
  name: string
  /**
   * (optional) A description of the value, such as an explaining why it exists or how to use it
   */
  description: string | null
} & ActionResultValue
export type ActionResultMemberInput = {
  /**
   * A human-readable name or title of the value, such as "Last Active" or "Login Password"
   */
  name: string
  /**
   * (optional) A description of the value, such as an explaining why it exists or how to use it
   */
  description?: string | null
} & ActionResultValueInput
export type ActionResultV0 = {
  message: string
  value: string | null
  copyable: boolean
  qr: boolean
}
export type ActionResultV0Input = {
  message: string
  value?: string | null
  copyable: boolean
  qr: boolean
}
export type ActionResultV1 = {
  /**
   * Primary text to display as the header of the response modal. e.g. "Success!", "Name Updated", or "Service Information", whatever makes sense
   */
  title: string
  /**
   * (optional) A general message for the user, just under the title. Rendered as Markdown, with a single line break kept as a line break.
   */
  message: string | null
  /**
   * (optional) Structured data to present inside the modal
   */
  result: ActionResultValue | null
}
export type ActionResultV1Input = {
  /**
   * Primary text to display as the header of the response modal. e.g. "Success!", "Name Updated", or "Service Information", whatever makes sense
   */
  title: string
  /**
   * (optional) A general message for the user, just under the title. Rendered as Markdown, with a single line break kept as a line break.
   */
  message?: string | null
  /**
   * (optional) Structured data to present inside the modal
   */
  result?: ActionResultValueInput | null
}
export type ActionResultValue =
  | ({ type: 'single' } & {
      /**
       * The actual string value to display. The UI renders it as a single-line field —
       * multi-line text belongs in a `multiline` value.
       */
      value: string
      /**
       * (optional) Whether or not to include a copy to clipboard icon to copy the value
       */
      copyable: boolean | null
      /**
       * (optional) Whether or not to also display the value as a QR code
       */
      qr: boolean | null
      /**
       * (optional) Whether or not to mask the value using ●●●●●●●, which is useful for password or other sensitive information
       */
      masked: boolean | null
      /**
       * (optional) Whether or not to include an open in new tab icon to launch the value, which must be an http(s) URL
       */
      launchable: boolean | null
    })
  | ({ type: 'multiline' } & {
      /**
       * The actual string value to display. The UI renders it verbatim in a read-only monospace field that keeps its line breaks
       */
      value: string
      /**
       * (optional) Whether or not to include a copy to clipboard icon to copy the value
       */
      copyable: boolean | null
      /**
       * (optional) Whether or not to also display the value as a QR code
       */
      qr: boolean | null
      /**
       * (optional) Whether or not to blur the value until the user reveals it, which is useful for a private key or other sensitive information
       */
      masked: boolean | null
      /**
       * (optional) Also offer the value as a download under this file name, such as "diagnostics.txt"
       */
      filename: string | null
    })
  | ({ type: 'group' } & {
      /**
       * An new group of nested values, experienced by the user as an accordion dropdown
       */
      value: ActionResultMember[]
    })
export type ActionResultValueInput =
  | ({ type: 'single' } & {
      /**
       * The actual string value to display. The UI renders it as a single-line field —
       * multi-line text belongs in a `multiline` value.
       */
      value: string
      /**
       * (optional) Whether or not to include a copy to clipboard icon to copy the value
       */
      copyable?: boolean | null
      /**
       * (optional) Whether or not to also display the value as a QR code
       */
      qr?: boolean | null
      /**
       * (optional) Whether or not to mask the value using ●●●●●●●, which is useful for password or other sensitive information
       */
      masked?: boolean | null
      /**
       * (optional) Whether or not to include an open in new tab icon to launch the value, which must be an http(s) URL
       */
      launchable?: boolean | null
    })
  | ({ type: 'multiline' } & {
      /**
       * The actual string value to display. The UI renders it verbatim in a read-only monospace field that keeps its line breaks
       */
      value: string
      /**
       * (optional) Whether or not to include a copy to clipboard icon to copy the value
       */
      copyable?: boolean | null
      /**
       * (optional) Whether or not to also display the value as a QR code
       */
      qr?: boolean | null
      /**
       * (optional) Whether or not to blur the value until the user reveals it, which is useful for a private key or other sensitive information
       */
      masked?: boolean | null
      /**
       * (optional) Also offer the value as a download under this file name, such as "diagnostics.txt"
       */
      filename?: string | null
    })
  | ({ type: 'group' } & {
      /**
       * An new group of nested values, experienced by the user as an accordion dropdown
       */
      value: ActionResultMemberInput[]
    })
export type ActionVisibility = 'hidden' | { disabled: string } | 'enabled'
export type ActionVisibilityInput = 'hidden' | { disabled: string } | 'enabled'
export type AddAdminParams = { signer: Guid }
export type AddAdminParamsInput = { signer: GuidInput }
export type AddAssetParams = {
  version: string
  platform: string
  url: string
  signature: AnySignature
  commitment: Blake3Commitment
}
export type AddAssetParamsInput = {
  version: string
  platform: string
  url: string
  signature: AnySignatureInput
  commitment: Blake3CommitmentInput
}
export type AddCategoryParams = { id: string; name: LocaleString }
export type AddCategoryParamsInput = { id: string; name: LocaleStringInput }
export type AddMirrorParams = {
  url: string
  commitment: MerkleArchiveCommitment
  signature: AnySignature
}
export type AddMirrorParamsInput = {
  url: string
  commitment: MerkleArchiveCommitmentInput
  signature: AnySignatureInput
}
export type AddPackageParams = {
  urls: string[]
  commitment: MerkleArchiveCommitment
  signature: AnySignature
}
export type AddPackageParamsInput = {
  urls: string[]
  commitment: MerkleArchiveCommitmentInput
  signature: AnySignatureInput
}
export type AddPackageSignerParams = {
  id: PackageId
  signer: Guid
  versions: string | null
  merge: boolean
}
export type AddPackageSignerParamsInput = {
  id: PackageIdInput
  signer: GuidInput
  versions?: string | null
  merge: boolean
}
export type AddPackageToCategoryParams = { id: string; package: PackageId }
export type AddPackageToCategoryParamsInput = {
  id: string
  package: PackageIdInput
}
export type AddPrivateDomainParams = { fqdn: string; gateway: GatewayId }
export type AddPrivateDomainParamsInput = {
  fqdn: string
  gateway: GatewayIdInput
}
export type AddPublicDomainParams = {
  fqdn: string
  acme: AcmeProvider | null
  gateway: GatewayId
  internalPort: number
}
export type AddPublicDomainParamsInput = {
  fqdn: string
  acme?: AcmeProviderInput | null
  gateway: GatewayIdInput
  internalPort: number
}
export type AddPublicDomainRes = {
  dns: QueryDnsRes
  port: CheckPortRes
  portV6: CheckPortV6Res | null
  /**
   * The authority's own reachability requirement, where the domain has one.
   */
  challenge: CheckChallengeRes | null
}
export type AddPublicDomainResInput = {
  dns: QueryDnsResInput
  port: CheckPortResInput
  portV6?: CheckPortV6ResInput | null
  /**
   * The authority's own reachability requirement, where the domain has one.
   */
  challenge?: CheckChallengeResInput | null
}
export type AddSslOptions = {
  preferredExternalPort: number
  /**
   * When `true`, the OS reverse proxy adds `X-Forwarded-Proto: https`
   * and `X-Forwarded-For: <client-ip>` to incoming HTTP requests before
   * forwarding them upstream. Setting this implies HTTP-aware proxying.
   */
  addXForwardedHeaders: boolean
  /**
   * The application protocols StartOS answers a client with, from those it
   * asked for. Unset answers with whatever it asked for.
   */
  alpn: AlpnInfo | null
  /**
   * Certificate validation for the OS→container TLS leg when rewrapping.
   * `None` (the default) validates against the StartOS root CA.
   */
  upstreamCertValidation: UpstreamCertValidation | null
  /**
   * Optional reverse-proxy auth gate. When set, the OS reverse proxy
   * will validate the `Authorization` header on incoming HTTP requests
   * against this configuration before forwarding them upstream.
   * Unauthenticated requests get `401 Unauthorized` with an appropriate
   * `WWW-Authenticate` challenge. For `Basic`, the authenticated
   * username is forwarded to the upstream service as `X-Forwarded-User`.
   * Setting this implies HTTP-aware proxying.
   */
  auth: ProxyAuth | null
}
export type AddSslOptionsInput = {
  preferredExternalPort: number
  /**
   * When `true`, the OS reverse proxy adds `X-Forwarded-Proto: https`
   * and `X-Forwarded-For: <client-ip>` to incoming HTTP requests before
   * forwarding them upstream. Setting this implies HTTP-aware proxying.
   */
  addXForwardedHeaders?: boolean
  /**
   * The application protocols StartOS answers a client with, from those it
   * asked for. Unset answers with whatever it asked for.
   */
  alpn?: CompatibleAlpnInfoInput | null
  /**
   * Certificate validation for the OS→container TLS leg when rewrapping.
   * `None` (the default) validates against the StartOS root CA.
   */
  upstreamCertValidation?: UpstreamCertValidationInput | null
  /**
   * Optional reverse-proxy auth gate. When set, the OS reverse proxy
   * will validate the `Authorization` header on incoming HTTP requests
   * against this configuration before forwarding them upstream.
   * Unauthenticated requests get `401 Unauthorized` with an appropriate
   * `WWW-Authenticate` challenge. For `Basic`, the authenticated
   * username is forwarded to the upstream service as `X-Forwarded-User`.
   * Setting this implies HTTP-aware proxying.
   */
  auth?: ProxyAuthInput | null
}
export type AddTunnelParams = {
  name: string
  config: string
  type: GatewayType | null
  setAsDefaultOutbound: boolean
}
export type AddTunnelParamsInput = {
  name: string
  config: string
  type?: GatewayTypeInput | null
  setAsDefaultOutbound: boolean
}
export type AddVersionParams = {
  version: string
  headline: string
  releaseNotes: string
  sourceVersion: string
}
export type AddVersionParamsInput = {
  version: string
  headline: string
  releaseNotes: string
  sourceVersion: string
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
export type Algorithm = 'ecdsa' | 'ed25519'
export type AlgorithmInput = 'ecdsa' | 'ed25519'
export type AllPackageData = { [key: string]: PackageDataEntry }
export type AllPackageDataInput = { [key: string]: PackageDataEntryInput }
export type AllowedStatuses = 'only-running' | 'only-stopped' | 'any'
export type AllowedStatusesInput = 'only-running' | 'only-stopped' | 'any'

/**
 * The protocols a binding answers with, carried on the wire as the list itself.
 */
export type AlpnInfo = MaybeUtf8String[]

/**
 * The protocols a binding answers with, carried on the wire as the list itself.
 */
export type AlpnInfoInput = MaybeUtf8StringInput[]
export type AnySignature = string
export type AnySignatureInput = string
export type AnySigningKey = string
export type AnySigningKeyInput = string
export type AnyVerifyingKey = string
export type AnyVerifyingKeyInput = string
export type ApiState = 'error' | 'initializing' | 'running'
export type ApiStateInput = 'error' | 'initializing' | 'running'
export type AttachParams = {
  password: EncryptedWire | null
  guid: string
  kiosk: boolean
}
export type AttachParamsInput = {
  password?: EncryptedWireInput | null
  guid: string
  kiosk: boolean
}

/**
 * The server's enrolled auth keys, keyed by their PEM encoding. Each enrolled
 * key is a sign-in: it carries the same metadata a session used to (when it
 * was created, when it was last used, and the user agent that enrolled it).
 */
export type AuthKeys = { [key: string]: Session }

/**
 * The server's enrolled auth keys, keyed by their PEM encoding. Each enrolled
 * key is a sign-in: it carries the same metadata a session used to (when it
 * was created, when it was last used, and the user agent that enrolled it).
 */
export type AuthKeysInput = { [key: string]: SessionInput }
export type BackupInfo = {
  version: string
  timestamp: string | null
  packageBackups: { [key: string]: PackageBackupInfo }
}
export type BackupInfoInput = {
  version: string
  timestamp?: string | null
  packageBackups: { [key: string]: PackageBackupInfoInput }
}
export type BackupParams = {
  targetId: BackupTargetId
  oldPassword: PasswordType | null
  packageIds: PackageId[] | null
  password: PasswordType
}
export type BackupParamsInput = {
  targetId: BackupTargetIdInput
  oldPassword?: PasswordTypeInput | null
  packageIds?: PackageIdInput[] | null
  password: PasswordTypeInput
}
export type BackupReport = {
  server: ServerBackupReport
  packages: { [key: string]: PackageBackupReport }
}
export type BackupReportInput = {
  server: ServerBackupReportInput
  packages: { [key: string]: PackageBackupReportInput }
}
export type BackupTarget =
  | ({ type: 'disk' } & ({
      vendor: string | null
      model: string | null
    } & PartitionInfo))
  | (
      | ({ type: 'cifs' } & Exclude<CifsBackupTarget, null>)
      | (null extends CifsBackupTarget ? { type: 'cifs' } : never)
    )
export type BackupTargetFS =
  | (
      | ({ type: 'disk' } & Exclude<{ logicalname: string }, null>)
      | (null extends { logicalname: string } ? { type: 'disk' } : never)
    )
  | (
      | ({ type: 'cifs' } & Exclude<Cifs, null>)
      | (null extends Cifs ? { type: 'cifs' } : never)
    )
export type BackupTargetFSInput =
  | (
      | ({ type: 'disk' } & Exclude<{ logicalname: string }, null>)
      | (null extends { logicalname: string } ? { type: 'disk' } : never)
    )
  | (
      | ({ type: 'cifs' } & Exclude<CifsInput, null>)
      | (null extends CifsInput ? { type: 'cifs' } : never)
    )
export type BackupTargetId = string
export type BackupTargetIdInput = string
export type BackupTargetInput =
  | ({ type: 'disk' } & ({
      vendor?: string | null
      model?: string | null
    } & PartitionInfoInput))
  | (
      | ({ type: 'cifs' } & Exclude<CifsBackupTargetInput, null>)
      | (null extends CifsBackupTargetInput ? { type: 'cifs' } : never)
    )
export type Base64 = string
export type Base64Input = string
export type BasicCredential = { username: string; password: string }
export type BasicCredentialInput = { username: string; password: string }
export type BindId = { id: HostId; internalPort: number }
export type BindIdInput = { id: HostIdInput; internalPort: number }
export type BindInfo = {
  enabled: boolean
  options: BindOptions
  net: NetInfo
  addresses: DerivedAddressInfo
  /**
   * Service interfaces exported from this binding (`Origin.export`). A single
   * binding (host + internal port) may back several interfaces (e.g. a `ui`
   * and an `api` on the same port), so this is keyed by interface id.
   */
  interfaces: { [key: string]: ServiceInterface }
}
export type BindInfoInput = {
  enabled: boolean
  options: BindOptionsInput
  net: NetInfoInput
  addresses: DerivedAddressInfoInput
  /**
   * Service interfaces exported from this binding (`Origin.export`). A single
   * binding (host + internal port) may back several interfaces (e.g. a `ui`
   * and an `api` on the same port), so this is keyed by interface id.
   */
  interfaces?: { [key: string]: ServiceInterfaceInput }
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
export type BindParams = { id: HostId; internalPort: number } & BindOptions
export type BindParamsInput = {
  id: HostIdInput
  internalPort: number
} & BindOptionsInput
export type BindRangeParams = {
  id: HostId
  internalStartPort: number
  externalStartPort: number
  numberOfPorts: number
}
export type BindRangeParamsInput = {
  id: HostIdInput
  internalStartPort: number
  externalStartPort: number
  numberOfPorts: number
}
export type BindingRanges = { [key: string]: RangeBindInfo }
export type BindingRangesInput = { [key: string]: RangeBindInfoInput }
export type BindingSetAddressEnabledParams = {
  internalPort: number
  address: HostnameInfo
  enabled: boolean | null
}
export type BindingSetAddressEnabledParamsInput = {
  internalPort: number
  address: HostnameInfoInput
  enabled?: boolean | null
}
export type BindingSetGuaWanParams = {
  internalPort: number
  address: HostnameInfo
  wan: boolean
}
export type BindingSetGuaWanParamsInput = {
  internalPort: number
  address: HostnameInfoInput
  wan: boolean
}
export type Bindings = { [key: string]: BindInfo }
export type BindingsInput = { [key: string]: BindInfoInput }
export type Blake3Commitment = { hash: string; size: number }
export type Blake3CommitmentInput = { hash: string; size: number }
export type BlockDev = { logicalname: string }
export type BlockDevInput = { logicalname: string }
export type BuildArg = string | { env: string }
export type BuildArgInput = string | { env: string }
export type CallbackId = number
export type CallbackIdInput = number
export type CancelInstallParams = { id: PackageId }
export type CancelInstallParamsInput = { id: PackageIdInput }
export type CapabilityVerdict = {
  /**
   * `None` = never probed.
   */
  supported: boolean | null
  at: string | null
}
export type CapabilityVerdictInput = {
  /**
   * `None` = never probed.
   */
  supported?: boolean | null
  at?: string | null
}
export type Category = { name: LocaleString }
export type CategoryInput = { name: LocaleStringInput }
export type Celsius = { value: string; unit: string }
export type CelsiusInput = { value: string; unit: string }
export type CheckChallengeParams = {
  fqdn: string
  gateway: GatewayId
  acme: AcmeProvider
}
export type CheckChallengeParamsInput = {
  fqdn: string
  gateway: GatewayIdInput
  acme: AcmeProviderInput
}

/**
 * Reachability of the port a certificate authority validates on. A null leg
 * went unprobed.
 */
export type CheckChallengeRes = {
  port: CheckPortRes | null
  portV6: CheckPortV6Res | null
}

/**
 * Reachability of the port a certificate authority validates on. A null leg
 * went unprobed.
 */
export type CheckChallengeResInput = {
  port?: CheckPortResInput | null
  portV6?: CheckPortV6ResInput | null
}
export type CheckDependenciesParam = { packageIds: PackageId[] | null }
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
export type CheckDependenciesResultInput = {
  packageId: PackageIdInput
  title?: string | null
  installedVersion?: VersionInput | null
  satisfies: VersionInput[]
  isRunning: boolean
  tasks: { [key: string]: TaskEntryInput }
  healthChecks: { [key: string]: NamedHealthCheckResultInput }
}
export type CheckDnsParams = { gateway: GatewayId; fqdn: string }
export type CheckDnsParamsInput = { gateway: GatewayIdInput; fqdn: string }
export type CheckPortParams = { port: number; gateway: GatewayId }
export type CheckPortParamsInput = { port: number; gateway: GatewayIdInput }
export type CheckPortRes = {
  ip: string
  port: number
  openExternally: boolean
  openInternally: boolean
  hairpinning: boolean
}
export type CheckPortResInput = {
  ip: string
  port: number
  openExternally: boolean
  openInternally: boolean
  hairpinning: boolean
}

/**
 * v6 reachability of the box's GUA at a port. v6 is NAT-free (the GUA is the
 * box's own address), so there is no hairpinning. Queried separately from
 * [`CheckPortRes`] so the IPv4 and IPv6 checks can run independently.
 */
export type CheckPortV6Res = {
  ip: string
  openExternally: boolean
  openInternally: boolean
}

/**
 * v6 reachability of the box's GUA at a port. v6 is NAT-free (the GUA is the
 * box's own address), so there is no hairpinning. Queried separately from
 * [`CheckPortRes`] so the IPv4 and IPv6 checks can run independently.
 */
export type CheckPortV6ResInput = {
  ip: string
  openExternally: boolean
  openInternally: boolean
}
export type Cifs = {
  hostname: string
  path: string
  username: string
  password: string | null
}
export type CifsAddParams = {
  hostname: string
  path: string
  username: string
  password: string | null
}
export type CifsAddParamsInput = {
  hostname: string
  path: string
  username: string
  password?: string | null
}
export type CifsBackupTarget = {
  hostname: string
  path: string
  username: string
  mountable: boolean
  available: number | null
  startOs: { [key: string]: StartOsRecoveryInfo }
  legacyBackup: boolean
}
export type CifsBackupTargetInput = {
  hostname: string
  path: string
  username: string
  mountable: boolean
  available?: number | null
  startOs: { [key: string]: StartOsRecoveryInfoInput }
  legacyBackup: boolean
}
export type CifsInput = {
  hostname: string
  path: string
  username: string
  password?: string | null
}
export type CifsRemoveParams = { id: BackupTargetId }
export type CifsRemoveParamsInput = { id: BackupTargetIdInput }
export type CifsUpdateParams = {
  id: BackupTargetId
  hostname: string
  path: string
  username: string
  password: string | null
}
export type CifsUpdateParamsInput = {
  id: BackupTargetIdInput
  hostname: string
  path: string
  username: string
  password?: string | null
}
export type ClearActionsParams = { except: ActionId[] }
export type ClearActionsParamsInput = { except: ActionIdInput[] }
export type ClearBindingsParams = { except: BindId[] }
export type ClearBindingsParamsInput = { except?: BindIdInput[] }
export type ClearCallbacksParams = { only: number[] } | { except: number[] }
export type ClearCallbacksParamsInput =
  | { only: number[] }
  | { except: number[] }
export type ClearServiceInterfacesParams = { except: ServiceInterfaceId[] }
export type ClearServiceInterfacesParamsInput = {
  except: ServiceInterfaceIdInput[]
}
export type ClearTaskParams = {
  packageId: PackageId
  replayId: ReplayId
  force: boolean
}
export type ClearTaskParamsInput = {
  packageId: PackageIdInput
  replayId: ReplayIdInput
  force?: boolean
}
export type ClearTasksParams = { only: string[] } | { except: string[] }
export type ClearTasksParamsInput = { only: string[] } | { except: string[] }
export type CliSetDescriptionParams = {
  description: LocaleString | null
  clear: boolean
}
export type CliSetDescriptionParamsInput = {
  description?: LocaleStringInput | null
  clear: boolean
}
export type CliSetIconParams = { icon: string | null; clear: boolean }
export type CliSetIconParamsInput = { icon?: string | null; clear: boolean }
export type CompatibleAlpnInfoInput =
  | MaybeUtf8StringInput[]
  | LegacyAlpnInfoInput
export type ContactInfo =
  | { email: string }
  | { matrix: string }
  | { website: string }
export type ContactInfoInput =
  | { email: string }
  | { matrix: string }
  | { website: string }
export type ControlParams = { id: PackageId }
export type ControlParamsInput = { id: PackageIdInput }
export type CountEntry = { label: string; count: number }
export type CountEntryInput = { label: string; count: number }
export type CreateNotificationParams = {
  level: NotificationLevel
  title: string
  message: string
  /**
   * Optional long-form, markdown-formatted body that the UI renders in a
   * "View Details" modal when present (release notes, post-update
   * changelogs, structured error reports). When omitted, the notification
   * has no extra payload.
   */
  data: string | null
}
export type CreateNotificationParamsInput = {
  level: NotificationLevelInput
  title: string
  message: string
  /**
   * Optional long-form, markdown-formatted body that the UI renders in a
   * "View Details" modal when present (release notes, post-update
   * changelogs, structured error reports). When omitted, the notification
   * has no extra payload.
   */
  data?: string | null
}
export type CreateSubcontainerFsParams = {
  imageId: ImageId
  name: string | null
}
export type CreateSubcontainerFsParamsInput = {
  imageId: ImageIdInput
  name?: string | null
}
export type CreateTaskParams = { replayId: ReplayId } & Task
export type CreateTaskParamsInput = { replayId: ReplayIdInput } & TaskParams
export type CurrentDependencies = { [key: string]: CurrentDependencyInfo }
export type CurrentDependenciesInput = {
  [key: string]: CurrentDependencyInfoInput
}
export type CurrentDependencyInfo = {
  title: LocaleString | null
  icon: DataUrl | null
  versionRange: string
} & CurrentDependencyKind
export type CurrentDependencyInfoInput = {
  title?: LocaleStringInput | null
  icon?: DataUrlInput | null
  versionRange: string
} & CurrentDependencyKindInput
export type CurrentDependencyKind =
  | { kind: 'exists' }
  | ({ kind: 'running' } & { healthChecks: string[] })
export type CurrentDependencyKindInput =
  | { kind: 'exists' }
  | ({ kind: 'running' } & { healthChecks?: string[] })
export type DataUrl = string
export type DataUrlInput = string
export type DeleteLegacyParams = { targetId: BackupTargetId }
export type DeleteLegacyParamsInput = { targetId: BackupTargetIdInput }
export type DepInfo = {
  description: LocaleString | null
  optional: boolean
  versionRange: string | null
} & Partial<Exclude<CurrentDependencyKind | null, null>> &
  Partial<Exclude<MetadataSrc | null, null>>
export type DepInfoInput = {
  description?: LocaleStringInput | null
  optional: boolean
  versionRange?: string | null
} & Partial<Exclude<CurrentDependencyKindInput | null, null>> &
  Partial<Exclude<MetadataSrcInput | null, null>>
export type Dependencies = { [key: string]: DepInfo }
export type DependenciesInput = { [key: string]: DepInfoInput }
export type DependencyMetadata = {
  title: LocaleString | null
  icon: DataUrl | null
  description: LocaleString | null
  optional: boolean
  versionRange: string | null
} & Partial<Exclude<CurrentDependencyKind | null, null>>
export type DependencyMetadataInput = {
  title?: LocaleStringInput | null
  icon?: DataUrlInput | null
  description?: LocaleStringInput | null
  optional: boolean
  versionRange?: string | null
} & Partial<Exclude<CurrentDependencyKindInput | null, null>>
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
  /**
   * User override: enable these addresses (only for public IP & port)
   */
  enabled: string[]
  /**
   * User override: disable these addresses (only for domains and private IP & port)
   */
  disabled: [string, number][]
  /**
   * User override: IPv6 global-unicast addresses opted into WAN exposure.
   * Projected into `HostnameInfo.public` by `update_addresses`, so `public`
   * stays the single source of truth for WAN reachability (P2P address
   * selection, upstream pinholes). A GUA not in this set is LAN-only.
   */
  guaWan: string[]
  /**
   * User override: enable these private IPs regardless of their mDNS address.
   */
  lanEnabled: [string, number][]
  /**
   * COMPUTED: NetServiceData::update — all possible addresses for this binding
   */
  available: HostnameInfo[]
}
export type DerivedAddressInfoInput = {
  /**
   * User override: enable these addresses (only for public IP & port)
   */
  enabled: string[]
  /**
   * User override: disable these addresses (only for domains and private IP & port)
   */
  disabled: [string, number][]
  /**
   * User override: IPv6 global-unicast addresses opted into WAN exposure.
   * Projected into `HostnameInfo.public` by `update_addresses`, so `public`
   * stays the single source of truth for WAN reachability (P2P address
   * selection, upstream pinholes). A GUA not in this set is LAN-only.
   */
  guaWan?: string[]
  /**
   * User override: enable these private IPs regardless of their mDNS address.
   */
  lanEnabled?: [string, number][]
  /**
   * COMPUTED: NetServiceData::update — all possible addresses for this binding
   */
  available: HostnameInfoInput[]
}
export type Description = { short: LocaleString; long: LocaleString }
export type DescriptionInput = {
  short: LocaleStringInput
  long: LocaleStringInput
}
export type DesiredStatus =
  | { main: 'stopped' }
  | ({ main: 'restarting' } & { restartAgain: boolean })
  | { main: 'running' }
  | ({ main: 'backing-up' } & { onComplete: StartStop })
  | ({ main: 'updating' } & { onComplete: StartStop })
export type DesiredStatusInput =
  | { main: 'stopped' }
  | ({ main: 'restarting' } & { restartAgain?: boolean })
  | { main: 'running' }
  | ({ main: 'backing-up' } & { onComplete: StartStopInput })
  | ({ main: 'updating' } & { onComplete: StartStopInput })
export type DestroySubcontainerFsParams = { guid: Guid }
export type DestroySubcontainerFsParamsInput = { guid: GuidInput }
export type DeviceFilter = {
  description: string
  class: 'processor' | 'display'
  product: string | null
  vendor: string | null
  capabilities: string[] | null
  driver: string | null
}
export type DeviceFilterInput = {
  description: string
  class: 'processor' | 'display'
  product?: string | null
  vendor?: string | null
  capabilities?: string[] | null
  driver?: string | null
}
export type DnsSettings = {
  dhcpServers: string[]
  staticServers: string[] | null
}
export type DnsSettingsInput = {
  dhcpServers: string[]
  staticServers?: string[] | null
}
export type DomainSettings = { gateway: GatewayId }
export type DomainSettingsInput = { gateway: GatewayIdInput }
export type DownloadsResponse = {
  totalRequests: number
  byPackage: CountEntry[]
  byPackageVersion: PackageVersionCount[]
}
export type DownloadsResponseInput = {
  totalRequests: number
  byPackage: CountEntryInput[]
  byPackageVersion: PackageVersionCountInput[]
}
export type Duration = string
export type DurationInput = string
export type EchoParams = { message: string }
export type EchoParamsInput = { message: string }
export type EditSignerParams = {
  id: Guid
  setName: string | null
  addContact: ContactInfo[]
  addKey: AnyVerifyingKey[]
  removeContact: ContactInfo[]
  removeKey: AnyVerifyingKey[]
}
export type EditSignerParamsInput = {
  id: GuidInput
  setName?: string | null
  addContact: ContactInfoInput[]
  addKey: AnyVerifyingKeyInput[]
  removeContact: ContactInfoInput[]
  removeKey: AnyVerifyingKeyInput[]
}
export type EffectsRunActionParams = {
  packageId: PackageId | null
  actionId: ActionId
  input: unknown
}
export type EffectsRunActionParamsInput = {
  packageId?: PackageIdInput | null
  actionId: ActionIdInput
  input: unknown
}
export type EncryptedWire = { encrypted: unknown }
export type EncryptedWireInput = { encrypted: unknown }

/**
 * Selects how aggressively an EPP-capable CPU pursues performance.
 */
export type Epp = string

/**
 * Selects how aggressively an EPP-capable CPU pursues performance.
 */
export type EppInput = string
export type ErrorData = { details: string; debug: string; info: unknown }
export type ErrorDataInput = { details: string; debug: string; info?: unknown }
export type ExportActionParams = { id: ActionId; metadata: ActionMetadata }
export type ExportActionParamsInput = {
  id: ActionIdInput
  metadata: ActionMetadataInput
}
export type ExportRangeServiceInterfaceParams = {
  hostId: HostId
  internalStartPort: number
  id: ServiceInterfaceId
  name: string
  description: string
  scheme: string | null
}
export type ExportRangeServiceInterfaceParamsInput = {
  hostId: HostIdInput
  internalStartPort: number
  id: ServiceInterfaceIdInput
  name: string
  description: string
  scheme?: string | null
}
export type ExportServiceInterfaceParams = {
  id: ServiceInterfaceId
  name: string
  description: string
  masked: boolean
  addressInfo: AddressInfo
  type: ServiceInterfaceType
  /**
   * The interface address Open UI should prefer.
   */
  preferredLauncherAddress: string | null
}
export type ExportServiceInterfaceParamsInput = {
  id: ServiceInterfaceIdInput
  name: string
  description: string
  masked: boolean
  addressInfo: AddressInfoInput
  type: ServiceInterfaceTypeInput
  /**
   * The interface address Open UI should prefer.
   */
  preferredLauncherAddress?: string | null
}
export type FileType = 'file' | 'directory' | 'infer'
export type FileTypeInput = 'file' | 'directory' | 'infer'
export type ForgetGatewayParams = { gateway: GatewayId }
export type ForgetGatewayParamsInput = { gateway: GatewayIdInput }
export type FullIndex = {
  name: string | null
  icon: DataUrl | null
  description: LocaleString | null
  package: PackageIndex
  os: OsIndex
  signers: { [key: string]: SignerInfo }
}
export type FullIndexInput = {
  name?: string | null
  icon?: DataUrlInput | null
  description?: LocaleStringInput | null
  package: PackageIndexInput
  os: OsIndexInput
  signers: { [key: string]: SignerInfoInput }
}
export type FullProgress = { overall: Progress; phases: NamedProgress[] }
export type FullProgressInput = {
  overall: ProgressInput
  phases: NamedProgressInput[]
}
export type GatewayId = string
export type GatewayIdInput = string
export type GatewayInfo = { id: GatewayId; name: string; public: boolean }
export type GatewayInfoInput = {
  id: GatewayIdInput
  name: string
  public: boolean
}

/**
 * Whether the gateway reachable via this interface speaks each port-mapping
 * protocol, as last probed. Fed by the watcher's periodic probes and by
 * failure/success evidence from the port-map client, and synced to the db so a
 * chronically uncooperative gateway is visible (and skipped) instead of being
 * retried forever.
 */
export type GatewayPortMapCapabilities = {
  pcp: CapabilityVerdict
  natPmp: CapabilityVerdict
  upnp: CapabilityVerdict
  /**
   * The PCP server answers ANNOUNCE with the Start9 capability marker, i.e.
   * it honors OPTION_HOSTNAME (SNI demux).
   */
  pcpHostname: CapabilityVerdict
}

/**
 * Whether the gateway reachable via this interface speaks each port-mapping
 * protocol, as last probed. Fed by the watcher's periodic probes and by
 * failure/success evidence from the port-map client, and synced to the db so a
 * chronically uncooperative gateway is visible (and skipped) instead of being
 * retried forever.
 */
export type GatewayPortMapCapabilitiesInput = {
  pcp: CapabilityVerdictInput
  natPmp: CapabilityVerdictInput
  upnp: CapabilityVerdictInput
  /**
   * The PCP server answers ANNOUNCE with the Start9 capability marker, i.e.
   * it honors OPTION_HOSTNAME (SNI demux).
   */
  pcpHostname: CapabilityVerdictInput
}
export type GatewayType = 'inbound-outbound' | 'outbound-only'
export type GatewayTypeInput = 'inbound-outbound' | 'outbound-only'
export type GenerateCertificateParams = {
  hostnames: string[]
  ed25519: boolean
}
export type GenerateCertificateParamsInput = {
  hostnames: string[]
  ed25519?: boolean
}
export type GenerateCertificateResponse = { key: string; fullchain: string }
export type GenerateCertificateResponseInput = {
  key: string
  fullchain: string
}
export type GetActionInputParams = {
  packageId: PackageId | null
  actionId: ActionId
  prefill: Record<string, unknown> | null
}
export type GetActionInputParamsInput = {
  packageId?: PackageIdInput | null
  actionId: ActionIdInput
  prefill?: Record<string, unknown> | null
}
export type GetContainerIpParams = {
  packageId: PackageId | null
  callback: CallbackId | null
}
export type GetContainerIpParamsInput = {
  packageId?: PackageIdInput | null
  callback?: CallbackIdInput | null
}
export type GetDownloadsParams = {
  /**
   * Filter by package ID
   */
  pkgId: string | null
  /**
   * Filter by version
   */
  version: string | null
  /**
   * Start of time range (RFC 3339)
   */
  after: string | null
  /**
   * End of time range (RFC 3339)
   */
  before: string | null
}
export type GetDownloadsParamsInput = {
  /**
   * Filter by package ID
   */
  pkgId?: string | null
  /**
   * Filter by version
   */
  version?: string | null
  /**
   * Start of time range (RFC 3339)
   */
  after?: string | null
  /**
   * End of time range (RFC 3339)
   */
  before?: string | null
}
export type GetHostInfoParams = {
  hostId: HostId
  packageId: PackageId | null
  callback: CallbackId | null
}
export type GetHostInfoParamsInput = {
  hostId: HostIdInput
  packageId?: PackageIdInput | null
  callback?: CallbackIdInput | null
}
export type GetOsAssetParams = { version: string; platform: string }
export type GetOsAssetParamsInput = { version: string; platform: string }
export type GetOsVersionParams = {
  sourceVersion: string | null
  targetVersion: string | null
  serverId: string | null
  platform: string | null
}
export type GetOsVersionParamsInput = {
  sourceVersion?: string | null
  targetVersion?: string | null
  serverId?: string | null
  platform?: string | null
}
export type GetOutboundGatewayParams = { callback: CallbackId | null }
export type GetOutboundGatewayParamsInput = {
  callback?: CallbackIdInput | null
}
export type GetPackageParams = {
  id: PackageId | null
  targetVersion: string | null
  sourceVersion: Version | null
  otherVersions: PackageDetailLevel | null
  allRevisions: boolean
}
export type GetPackageParamsInput = {
  id?: PackageIdInput | null
  targetVersion?: string | null
  sourceVersion?: VersionInput | null
  otherVersions?: PackageDetailLevelInput | null
  allRevisions?: boolean
}
export type GetPackageResponse = {
  categories: string[]
  best: { [key: string]: PackageVersionInfo }
  otherVersions?: { [key: string]: PackageInfoShort } | null
}
export type GetPackageResponseFull = {
  categories: string[]
  best: { [key: string]: PackageVersionInfo }
  otherVersions: { [key: string]: PackageVersionInfo }
}
export type GetPackageResponseFullInput = {
  categories: string[]
  best: { [key: string]: PackageVersionInfoInput }
  otherVersions: { [key: string]: PackageVersionInfoInput }
}
export type GetPackageResponseInput = {
  categories: string[]
  best: { [key: string]: PackageVersionInfoInput }
  otherVersions?: { [key: string]: PackageInfoShortInput } | null
}
export type GetServiceInterfaceParams = {
  packageId: PackageId | null
  serviceInterfaceId: ServiceInterfaceId
  callback: CallbackId | null
}
export type GetServiceInterfaceParamsInput = {
  packageId?: PackageIdInput | null
  serviceInterfaceId: ServiceInterfaceIdInput
  callback?: CallbackIdInput | null
}
export type GetServiceManifestParams = {
  packageId: PackageId
  callback: CallbackId | null
}
export type GetServiceManifestParamsInput = {
  packageId: PackageIdInput
  callback?: CallbackIdInput | null
}
export type GetServicePortForwardParams = {
  packageId: PackageId | null
  hostId: HostId
  internalPort: number
}
export type GetServicePortForwardParamsInput = {
  packageId?: PackageIdInput | null
  hostId: HostIdInput
  internalPort: number
}
export type GetSslCertificateParams = {
  hostnames: string[]
  algorithm: Algorithm | null
  callback: CallbackId | null
}
export type GetSslCertificateParamsInput = {
  hostnames: string[]
  algorithm?: AlgorithmInput | null
  callback?: CallbackIdInput | null
}
export type GetSslKeyParams = {
  hostnames: string[]
  algorithm: Algorithm | null
}
export type GetSslKeyParamsInput = {
  hostnames: string[]
  algorithm?: AlgorithmInput | null
}
export type GetStatusParams = {
  packageId: PackageId | null
  callback: CallbackId | null
}
export type GetStatusParamsInput = {
  packageId?: PackageIdInput | null
  callback?: CallbackIdInput | null
}
export type GetSystemSmtpParams = { callback: CallbackId | null }
export type GetSystemSmtpParamsInput = { callback?: CallbackIdInput | null }
export type GetUsersParams = {
  /**
   * Start of time range (RFC 3339)
   */
  after: string | null
  /**
   * End of time range (RFC 3339)
   */
  before: string | null
}
export type GetUsersParamsInput = {
  /**
   * Start of time range (RFC 3339)
   */
  after?: string | null
  /**
   * End of time range (RFC 3339)
   */
  before?: string | null
}
export type GigaBytes = { value: string; unit: string }
export type GigaBytesInput = { value: string; unit: string }
export type GitHash = string
export type GitHashInput = string
export type Governor = string
export type GovernorInput = string
export type Guid = string
export type GuidInput = string
export type HardwareRequirements = {
  device: DeviceFilter[]
  ram: number | null
  arch: string[] | null
}
export type HardwareRequirementsInput = {
  device?: DeviceFilterInput[]
  ram?: number | null
  arch?: string[] | null
}
export type HealthCheckId = string
export type HealthCheckIdInput = string
export type Host = {
  bindings: Bindings
  bindingRanges: BindingRanges
  publicDomains: { [key: string]: PublicDomainConfig }
  privateDomains: { [key: string]: GatewayId[] }
  /**
   * COMPUTED: port forwarding rules needed on gateways for public addresses to work.
   */
  portForwards: PortForward[]
}
export type HostId = string
export type HostIdInput = string
export type HostInput = {
  bindings: BindingsInput
  bindingRanges?: BindingRangesInput
  publicDomains: { [key: string]: PublicDomainConfigInput }
  privateDomains: { [key: string]: GatewayIdInput[] }
  /**
   * COMPUTED: port forwarding rules needed on gateways for public addresses to work.
   */
  portForwards?: PortForwardInput[]
}
export type HostnameInfo = {
  ssl: boolean
  public: boolean
  hostname: string
  port: number | null
  metadata: HostnameMetadata
}
export type HostnameInfoInput = {
  ssl: boolean
  public: boolean
  hostname: string
  port?: number | null
  metadata: HostnameMetadataInput
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
export type HostnameMetadataInput =
  | ({ kind: 'ipv4' } & { gateway: GatewayIdInput })
  | ({ kind: 'ipv6' } & { gateway: GatewayIdInput; scopeId: number })
  | ({ kind: 'mdns' } & { gateways: GatewayIdInput[] })
  | ({ kind: 'private-domain' } & { gateways: GatewayIdInput[] })
  | ({ kind: 'public-domain' } & { gateway: GatewayIdInput })
  | ({ kind: 'plugin' } & {
      packageId: PackageIdInput
      removeAction?: ActionIdInput | null
      overflowActions: ActionIdInput[]
      info?: unknown
    })
export type Hosts = { [key: string]: Host }
export type HostsInput = { [key: string]: HostInput }
export type IdMap = { fromId: number; toId: number; range: number }
export type IdMapInput = { fromId: number; toId: number; range: number }
export type ImageConfig = {
  source: ImageSource
  arch: string[]
  emulateMissing: boolean
  nvidiaContainer: boolean
}
export type ImageConfigInput = ImageConfigReprInput
export type ImageConfigReprInput = {
  source: ImageSourceInput
  arch: string[]
  emulateMissing?: boolean | null
  emulateMissingAs?: string | null
  nvidiaContainer?: boolean
}
export type ImageId = string
export type ImageIdInput = string
export type ImageMetadata = {
  workdir: string
  user: string
  entrypoint: string[] | null
  cmd: string[] | null
}
export type ImageMetadataInput = {
  workdir: string
  user: string
  entrypoint?: string[] | null
  cmd?: string[] | null
}
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
export type ImageSourceInput =
  | 'packed'
  | {
      dockerBuild: {
        workdir?: string | null
        dockerfile?: string | null
        buildArgs?: { [key: string]: BuildArgInput } | null
      }
    }
  | { dockerTag: string }
export type InfoParams = {
  targetId: BackupTargetId
  serverId: string
  password: string
}
export type InfoParamsInput = {
  targetId: BackupTargetIdInput
  serverId: string
  password: string
}
export type InitAcmeParams = { provider: AcmeProvider; contact: string[] }
export type InitAcmeParamsInput = {
  provider: AcmeProviderInput
  contact: string[]
}
export type InitProgressRes = { progress: FullProgress; guid: Guid }
export type InitProgressResInput = {
  progress: FullProgressInput
  guid: GuidInput
}
export type InstallParams = {
  registry: string
  id: PackageId
  version: Version
}
export type InstallParamsInput = {
  registry: string
  id: PackageIdInput
  version: VersionInput
}
export type InstalledState = { manifest: Manifest }
export type InstalledStateInput = { manifest: ManifestInput }
export type InstalledVersionParams = { id: PackageId }
export type InstalledVersionParamsInput = { id: PackageIdInput }
export type InstallingInfo = { newManifest: Manifest; progress: FullProgress }
export type InstallingInfoInput = {
  newManifest: ManifestInput
  progress: FullProgressInput
}
export type InstallingState = { installingInfo: InstallingInfo }
export type InstallingStateInput = { installingInfo: InstallingInfoInput }
export type IpInfo = {
  name: string
  scopeId: number
  deviceType: NetworkInterfaceType | null
  subnets: string[]
  lanIp: string[]
  wanIp: string | null
  ntpServers: string[]
  dnsServers: string[]
}
export type IpInfoInput = {
  name: string
  scopeId: number
  deviceType?: NetworkInterfaceTypeInput | null
  subnets: string[]
  lanIp: string[]
  wanIp?: string | null
  ntpServers: string[]
  dnsServers: string[]
}
export type KeyboardOptions = {
  layout: string
  keymap: string | null
  model: string | null
  variant: string | null
  options: string[]
}
export type KeyboardOptionsInput = {
  layout: string
  keymap?: string | null
  model?: string | null
  variant?: string | null
  options?: string[]
}
export type KillParams = { ids: string[] }
export type KillParamsInput = { ids: string[] }
export type LegacyAlpnInfoInput =
  | 'reflect'
  | { specified: MaybeUtf8StringInput[] }
export type ListNotificationParams = {
  before: number | null
  limit: number | null
}
export type ListNotificationParamsInput = {
  before?: number | null
  limit?: number | null
}
export type ListPackageSignersParams = { id: PackageId }
export type ListPackageSignersParamsInput = { id: PackageIdInput }
export type ListServiceInterfacesParams = {
  packageId: PackageId | null
  callback: CallbackId | null
}
export type ListServiceInterfacesParamsInput = {
  packageId?: PackageIdInput | null
  callback?: CallbackIdInput | null
}
export type ListVersionSignersParams = { version: string }
export type ListVersionSignersParamsInput = { version: string }
export type LocaleString = string | Record<string, string>
export type LocaleStringInput = string | Record<string, string>
export type LogEntry = { timestamp: string; message: string; bootId: string }
export type LogEntryInput = {
  timestamp: string
  message: string
  bootId: string
}
export type LogFollowResponse = { startCursor: string | null; guid: Guid }
export type LogFollowResponseInput = {
  startCursor?: string | null
  guid: GuidInput
}
export type LogResponse = {
  entries: LogEntry[]
  startCursor: string | null
  endCursor: string | null
}
export type LogResponseInput = {
  entries: LogEntryInput[]
  startCursor?: string | null
  endCursor?: string | null
}
export type LoginParams = {
  password: string
  /**
   * The PEM-encoded public key to enroll on a successful login. The login
   * request itself is signed with the matching secret key, so enrollment
   * proves possession.
   */
  pubkey: AnyVerifyingKey
  /**
   * Enroll in memory only, never persisted (kiosk mode, which re-enrolls
   * on every browser restart and would otherwise accumulate keys).
   */
  ephemeral: boolean
}
export type LoginParamsInput = {
  password: string
  /**
   * The PEM-encoded public key to enroll on a successful login. The login
   * request itself is signed with the matching secret key, so enrollment
   * proves possession.
   */
  pubkey: AnyVerifyingKeyInput
  /**
   * Enroll in memory only, never persisted (kiosk mode, which re-enrolls
   * on every browser restart and would otherwise accumulate keys).
   */
  ephemeral?: boolean
}
export type LogsParams = {
  limit: number | null
  cursor: string | null
  boot: number | string | null
  before: boolean
} & {}
export type LogsParamsInput = {
  limit?: number | null
  cursor?: string | null
  boot?: number | string | null
  before?: boolean
} & {}
export type LshwDevice =
  | (
      | ({ class: 'processor' } & Exclude<LshwProcessor, null>)
      | (null extends LshwProcessor ? { class: 'processor' } : never)
    )
  | (
      | ({ class: 'display' } & Exclude<LshwDisplay, null>)
      | (null extends LshwDisplay ? { class: 'display' } : never)
    )
export type LshwDeviceInput =
  | (
      | ({ class: 'processor' } & Exclude<LshwProcessorInput, null>)
      | (null extends LshwProcessorInput ? { class: 'processor' } : never)
    )
  | (
      | ({ class: 'display' } & Exclude<LshwDisplayInput, null>)
      | (null extends LshwDisplayInput ? { class: 'display' } : never)
    )
export type LshwDisplay = {
  product: string | null
  vendor: string | null
  capabilities: string[]
  driver: string | null
}
export type LshwDisplayInput = {
  product?: string | null
  vendor?: string | null
  capabilities: string[]
  driver?: string | null
}
export type LshwProcessor = {
  product: string | null
  vendor: string | null
  capabilities: string[]
}
export type LshwProcessorInput = {
  product?: string | null
  vendor?: string | null
  capabilities: string[]
}
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
export type ManifestInput = {
  id: PackageIdInput
  version: VersionInput
  canMigrateTo: string
  canMigrateFrom: string
  images: { [key: string]: ImageConfigInput }
  volumes: VolumeIdInput[]
  dependencies?: DependenciesInput
  hardwareRequirements?: HardwareRequirementsInput
} & PackageMetadataInput
export type MaybeUtf8String = string | number[]
export type MaybeUtf8StringInput = string | number[]
export type MebiBytes = { value: string; unit: string }
export type MebiBytesInput = { value: string; unit: string }
export type MerkleArchiveCommitment = {
  rootSighash: string
  rootMaxsize: number
}
export type MerkleArchiveCommitmentInput = {
  rootSighash: string
  rootMaxsize: number
}
export type Metadata = { title: LocaleString; icon: PathOrUrl }
export type MetadataInput = { title: LocaleStringInput; icon: PathOrUrlInput }
export type MetadataSrc = { metadata: Metadata } | { s9pk: PathOrUrl | null }
export type MetadataSrcInput =
  | { metadata: MetadataInput }
  | { s9pk: PathOrUrlInput | null }
export type Metrics = {
  general: MetricsGeneral
  memory: MetricsMemory
  cpu: MetricsCpu
  disk: MetricsDisk
}
export type MetricsCpu = {
  percentageUsed: Percentage
  idle: Percentage
  userSpace: Percentage
  kernelSpace: Percentage
  wait: Percentage
}
export type MetricsCpuInput = {
  percentageUsed: PercentageInput
  idle: PercentageInput
  userSpace: PercentageInput
  kernelSpace: PercentageInput
  wait: PercentageInput
}
export type MetricsDisk = {
  percentageUsed: Percentage
  used: GigaBytes
  available: GigaBytes
  capacity: GigaBytes
}
export type MetricsDiskInput = {
  percentageUsed: PercentageInput
  used: GigaBytesInput
  available: GigaBytesInput
  capacity: GigaBytesInput
}
export type MetricsFollowResponse = { guid: Guid; metrics: Metrics }
export type MetricsFollowResponseInput = {
  guid: GuidInput
  metrics: MetricsInput
}
export type MetricsGeneral = { temperature: Celsius | null }
export type MetricsGeneralInput = { temperature?: CelsiusInput | null }
export type MetricsInput = {
  general: MetricsGeneralInput
  memory: MetricsMemoryInput
  cpu: MetricsCpuInput
  disk: MetricsDiskInput
}
export type MetricsMemory = {
  percentageUsed: Percentage
  total: MebiBytes
  available: MebiBytes
  used: MebiBytes
  zramTotal: MebiBytes
  zramAvailable: MebiBytes
  zramUsed: MebiBytes
}
export type MetricsMemoryInput = {
  percentageUsed: PercentageInput
  total: MebiBytesInput
  available: MebiBytesInput
  used: MebiBytesInput
  zramTotal: MebiBytesInput
  zramAvailable: MebiBytesInput
  zramUsed: MebiBytesInput
}
export type MetricsSummary = {
  totalCheckins: number
  uniqueServers: number
  totalPackageRequests: number
  byArch: CountEntry[]
  byOsVersion: CountEntry[]
}
export type MetricsSummaryInput = {
  totalCheckins: number
  uniqueServers: number
  totalPackageRequests: number
  byArch: CountEntryInput[]
  byOsVersion: CountEntryInput[]
}
export type ModifyNotificationBeforeParams = { before: number }
export type ModifyNotificationBeforeParamsInput = { before: number }
export type ModifyNotificationParams = { ids: number[] }
export type ModifyNotificationParamsInput = { ids: number[] }
export type MountParams = { location: string; target: MountTarget }
export type MountParamsInput = { location: string; target: MountTargetInput }
export type MountTarget = {
  packageId: PackageId
  volumeId: VolumeId
  subpath: string | null
  readonly: boolean
  idmap: IdMap[]
}
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
export type NamedProgress = { name: string; progress: Progress }
export type NamedProgressInput = { name: string; progress: ProgressInput }
export type NetInfo = {
  assignedPort: number | null
  assignedSslPort: number | null
}
export type NetInfoInput = {
  assignedPort?: number | null
  assignedSslPort?: number | null
}
export type NetworkInfo = {
  wifi: WifiInfo
  host: Host
  gateways: { [key: string]: NetworkInterfaceInfo }
  acme: { [key: string]: AcmeSettings }
  dns: DnsSettings
  defaultOutbound: string | null
  passthroughs: PassthroughInfo[]
}
export type NetworkInfoInput = {
  wifi: WifiInfoInput
  host: HostInput
  gateways?: { [key: string]: NetworkInterfaceInfoInput }
  acme?: { [key: string]: AcmeSettingsInput }
  dns?: DnsSettingsInput
  defaultOutbound?: string | null
  passthroughs?: PassthroughInfoInput[]
}
export type NetworkInterfaceInfo = {
  name: string | null
  secure: boolean | null
  ipInfo: IpInfo | null
  type: GatewayType
  portMap: GatewayPortMapCapabilities
  /**
   * The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence
   * from the update client (`net::dns_update`). A WireGuard gateway only
   * serves the injected `<hostname>.local` while this is `supported`, so
   * only then is the name listed on it.
   */
  dnsUpdate: CapabilityVerdict
}
export type NetworkInterfaceInfoInput = {
  name?: string | null
  secure?: boolean | null
  ipInfo?: IpInfoInput | null
  type?: GatewayTypeInput | null
  portMap?: GatewayPortMapCapabilitiesInput
  /**
   * The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence
   * from the update client (`net::dns_update`). A WireGuard gateway only
   * serves the injected `<hostname>.local` while this is `supported`, so
   * only then is the name listed on it.
   */
  dnsUpdate?: CapabilityVerdictInput
}
export type NetworkInterfaceType =
  | 'ethernet'
  | 'wireless'
  | 'bridge'
  | 'wireguard'
  | 'loopback'
export type NetworkInterfaceTypeInput =
  | 'ethernet'
  | 'wireless'
  | 'bridge'
  | 'wireguard'
  | 'loopback'
export type Notification = {
  packageId: PackageId | null
  createdAt: string
  code: number
  level: NotificationLevel
  title: string
  message: string
  data: unknown
  seen: boolean
}
export type NotificationInput = {
  packageId?: PackageIdInput | null
  createdAt: string
  code: number
  level: NotificationLevelInput
  title: string
  message: string
  data: unknown
  seen?: boolean
}
export type NotificationLevel = 'success' | 'info' | 'warning' | 'error'
export type NotificationLevelInput = 'success' | 'info' | 'warning' | 'error'
export type NotificationWithId = { id: number } & Notification
export type NotificationWithIdInput = { id: number } & NotificationInput
export type OsIndex = { versions: OsVersionInfoMap }
export type OsIndexInput = { versions: OsVersionInfoMapInput }
export type OsVersionInfo = {
  headline: string
  releaseNotes: string
  sourceVersion: string
  authorized: Guid[]
  iso: { [key: string]: RegistryAsset<Blake3Commitment> }
  squashfs: { [key: string]: RegistryAsset<Blake3Commitment> }
  img: { [key: string]: RegistryAsset<Blake3Commitment> }
}
export type OsVersionInfoInput = {
  headline: string
  releaseNotes: string
  sourceVersion: string
  authorized: GuidInput[]
  iso: { [key: string]: RegistryAssetInput<Blake3CommitmentInput> }
  squashfs: { [key: string]: RegistryAssetInput<Blake3CommitmentInput> }
  img: { [key: string]: RegistryAssetInput<Blake3CommitmentInput> }
}
export type OsVersionInfoMap = { [key: string]: OsVersionInfo }
export type OsVersionInfoMapInput = { [key: string]: OsVersionInfoInput }
export type PackageBackupInfo = {
  title: string
  version: Version
  osVersion: string
  timestamp: string
}
export type PackageBackupInfoInput = {
  title: string
  version: VersionInput
  osVersion: string
  timestamp: string
}
export type PackageBackupReport = { error: string | null; duration_ms: number }
export type PackageBackupReportInput = {
  error?: string | null
  duration_ms: number
}
export type PackageDataEntry = {
  stateInfo: PackageState
  s9pk: string
  statusInfo: StatusInfo
  registry: string | null
  developerKey: string
  icon: DataUrl
  lastBackup: string | null
  currentDependencies: CurrentDependencies
  actions: { [key: string]: ActionMetadata }
  tasks: { [key: string]: TaskEntry }
  hosts: Hosts
  storeExposedDependents: string[]
  outboundGateway: string | null
  plugin: PackagePlugin
}
export type PackageDataEntryInput = {
  stateInfo: PackageStateInput
  s9pk: string
  statusInfo: StatusInfoInput
  registry?: string | null
  developerKey: string
  icon: DataUrlInput
  lastBackup?: string | null
  currentDependencies: CurrentDependenciesInput
  actions: { [key: string]: ActionMetadataInput }
  tasks: { [key: string]: TaskEntryInput }
  hosts: HostsInput
  storeExposedDependents: string[]
  outboundGateway?: string | null
  plugin?: PackagePluginInput
}
export type PackageDetailLevel = 'none' | 'short' | 'full'
export type PackageDetailLevelInput = 'none' | 'short' | 'full'
export type PackageId = string
export type PackageIdInput = string
export type PackageIndex = {
  categories: { [key: string]: Category }
  packages: { [key: string]: PackageInfo }
}
export type PackageIndexInput = {
  categories: { [key: string]: CategoryInput }
  packages: { [key: string]: PackageInfoInput }
}
export type PackageInfo = {
  authorized: { [key: string]: string }
  versions: { [key: string]: PackageVersionInfo }
  categories: string[]
}
export type PackageInfoInput = {
  authorized: { [key: string]: string }
  versions: { [key: string]: PackageVersionInfoInput }
  categories: string[]
}
export type PackageInfoShort = { releaseNotes: LocaleString }
export type PackageInfoShortInput = { releaseNotes: LocaleStringInput }
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
  /**
   * Grants access to `/dev/fuse`.
   */
  userspaceFilesystems: boolean
  /**
   * Grants access to `/dev/net/tun`.
   */
  virtualNetworking: boolean
  /**
   * Grants /dev/kvm when present. The opening process must run as container root.
   */
  hardwareVirtualization: boolean
  plugins: PluginId[]
  satisfies: Version[]
}
export type PackageMetadataInput = {
  title: string
  description: DescriptionInput
  releaseNotes: LocaleStringInput
  preDownloadAlert?: PreDownloadAlertInput | null
  gitHash?: GitHashInput | null
  license: string
  packageRepo?: string
  upstreamRepo: string
  marketingUrl?: string | null
  donationUrl?: string | null
  osVersion?: string
  sdkVersion?: string | null
  hardwareAcceleration?: boolean
  /**
   * Grants access to `/dev/fuse`.
   */
  userspaceFilesystems?: boolean
  /**
   * Grants access to `/dev/net/tun`.
   */
  virtualNetworking?: boolean
  /**
   * Grants /dev/kvm when present. The opening process must run as container root.
   */
  hardwareVirtualization?: boolean
  plugins?: PluginIdInput[]
  satisfies?: VersionInput[]
}
export type PackagePlugin = { url: UrlPluginRegistration | null }
export type PackagePluginInput = { url?: UrlPluginRegistrationInput | null }
export type PackageState =
  | (
      | ({ state: 'installing' } & Exclude<InstallingState, null>)
      | (null extends InstallingState ? { state: 'installing' } : never)
    )
  | (
      | ({ state: 'restoring' } & Exclude<InstallingState, null>)
      | (null extends InstallingState ? { state: 'restoring' } : never)
    )
  | (
      | ({ state: 'updating' } & Exclude<UpdatingState, null>)
      | (null extends UpdatingState ? { state: 'updating' } : never)
    )
  | (
      | ({ state: 'installed' } & Exclude<InstalledState, null>)
      | (null extends InstalledState ? { state: 'installed' } : never)
    )
  | (
      | ({ state: 'removing' } & Exclude<InstalledState, null>)
      | (null extends InstalledState ? { state: 'removing' } : never)
    )
export type PackageStateInput =
  | (
      | ({ state: 'installing' } & Exclude<InstallingStateInput, null>)
      | (null extends InstallingStateInput ? { state: 'installing' } : never)
    )
  | (
      | ({ state: 'restoring' } & Exclude<InstallingStateInput, null>)
      | (null extends InstallingStateInput ? { state: 'restoring' } : never)
    )
  | (
      | ({ state: 'updating' } & Exclude<UpdatingStateInput, null>)
      | (null extends UpdatingStateInput ? { state: 'updating' } : never)
    )
  | (
      | ({ state: 'installed' } & Exclude<InstalledStateInput, null>)
      | (null extends InstalledStateInput ? { state: 'installed' } : never)
    )
  | (
      | ({ state: 'removing' } & Exclude<InstalledStateInput, null>)
      | (null extends InstalledStateInput ? { state: 'removing' } : never)
    )
export type PackageVersionCount = {
  pkgId: string
  version: string
  count: number
}
export type PackageVersionCountInput = {
  pkgId: string
  version: string
  count: number
}
export type PackageVersionInfo = {
  icon: DataUrl
  dependencyMetadata: { [key: string]: DependencyMetadata }
  sourceVersion: string | null
  s9pks: [HardwareRequirements, RegistryAsset<MerkleArchiveCommitment>][]
} & PackageMetadata
export type PackageVersionInfoInput = {
  icon: DataUrlInput
  dependencyMetadata: { [key: string]: DependencyMetadataInput }
  sourceVersion?: string | null
  s9pks: [
    HardwareRequirementsInput,
    RegistryAssetInput<MerkleArchiveCommitmentInput>,
  ][]
} & PackageMetadataInput
export type PartitionInfo = {
  logicalname: string
  stablePath: string
  label: string | null
  capacity: number
  used: number | null
  available: number | null
  startOs: { [key: string]: StartOsRecoveryInfo }
  legacyBackup: boolean
  guid: string | null
  filesystem: string | null
}
export type PartitionInfoInput = {
  logicalname: string
  stablePath: string
  label?: string | null
  capacity: number
  used?: number | null
  available?: number | null
  startOs: { [key: string]: StartOsRecoveryInfoInput }
  legacyBackup: boolean
  guid?: string | null
  filesystem?: string | null
}
export type PassthroughInfo = {
  hostname: string
  listenPort: number
  backend: string
  publicGateways: string[]
  privateIps: string[]
}
export type PassthroughInfoInput = {
  hostname: string
  listenPort: number
  backend: string
  publicGateways: string[]
  privateIps: string[]
}
export type PasswordType = EncryptedWire | string
export type PasswordTypeInput = EncryptedWireInput | string
export type PathOrUrl = string
export type PathOrUrlInput = string
export type Pem = string
export type PemInput = string
export type Percentage = { value: string; unit: string }
export type PercentageInput = { value: string; unit: string }
export type PluginHostnameInfo = {
  /**
   * [`PackageId::start_os`] identifies the server's own host (the StartOS UI).
   */
  packageId: PackageId
  hostId: HostId
  internalPort: number
  ssl: boolean
  public: boolean
  hostname: string
  port: number | null
  info: unknown
}
export type PluginHostnameInfoInput = {
  /**
   * [`PackageId::start_os`] identifies the server's own host (the StartOS UI).
   */
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
export type PluginIdInput = 'url-v0'
export type PortForward = {
  src: string
  dst: string
  gateway: GatewayId
  /**
   * Number of contiguous ports covered by this forward (always >= 1).
   * Set to >1 by [`crate::net::host::binding::RangeBindInfo`] entries to
   * represent a single iptables port-range rule rather than N per-port
   * rules. Defaults to 1 to keep the on-disk shape compatible with
   * existing single-port `PortForward`s.
   */
  count: number
  /**
   * Whether this forward receives local traffic. Used to gate hairpinning check.
   */
  local: boolean
}
export type PortForwardInput = {
  src: string
  dst: string
  gateway: GatewayIdInput
  /**
   * Number of contiguous ports covered by this forward (always >= 1).
   * Set to >1 by [`crate::net::host::binding::RangeBindInfo`] entries to
   * represent a single iptables port-range rule rather than N per-port
   * rules. Defaults to 1 to keep the on-disk shape compatible with
   * existing single-port `PortForward`s.
   */
  count?: number
  /**
   * Whether this forward receives local traffic. Used to gate hairpinning check.
   */
  local?: boolean
}
export type PreDownloadAlert = {
  message: LocaleString
  when: PreDownloadAlertWhen
}
export type PreDownloadAlertInput = {
  message: LocaleStringInput
  when: PreDownloadAlertWhenInput
}
export type PreDownloadAlertWhen = { sourceVersion: string }
export type PreDownloadAlertWhenInput = { sourceVersion: string }
export type Progress =
  | null
  | boolean
  | { done: number; total: number | null; units: ProgressUnits | null }
  /**
   * A sub-tree of progress. Used by `NamedProgress.progress` so a phase
   * can carry its own `FullProgress` with sub-phases. Producers are
   * expected to keep `FullProgress.overall` scalar (not nested again).
   */
  | FullProgress
export type ProgressInput =
  | null
  | boolean
  | { done: number; total?: number | null; units?: ProgressUnitsInput | null }
  /**
   * A sub-tree of progress. Used by `NamedProgress.progress` so a phase
   * can carry its own `FullProgress` with sub-phases. Producers are
   * expected to keep `FullProgress.overall` scalar (not nested again).
   */
  | FullProgressInput
export type ProgressUnits = 'bytes' | 'steps'
export type ProgressUnitsInput = 'bytes' | 'steps'

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
export type ProxyAuth =
  | ({ type: 'bearer' } & { tokens: string[]; realm: string | null })
  | ({ type: 'basic' } & {
      credentials: BasicCredential[]
      realm: string | null
    })

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
export type ProxyAuthInput =
  | ({ type: 'bearer' } & { tokens: string[]; realm?: string | null })
  | ({ type: 'basic' } & {
      credentials: BasicCredentialInput[]
      realm?: string | null
    })
export type Public = {
  serverInfo: ServerInfo
  packageData: AllPackageData
  ui: unknown
}
export type PublicDomainConfig = {
  gateway: GatewayId
  acme: AcmeProvider | null
}
export type PublicDomainConfigInput = {
  gateway: GatewayIdInput
  acme?: AcmeProviderInput | null
}
export type PublicInput = {
  serverInfo: ServerInfoInput
  packageData: AllPackageDataInput
  ui: unknown
}
export type QueryDnsParams = { fqdn: string }
export type QueryDnsParamsInput = { fqdn: string }

/**
 * What a public domain currently resolves to, per address family. A public
 * domain is DualStack: the operator points an `A` record at the gateway's WAN
 * IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
 */
export type QueryDnsRes = { ipv4: string | null; ipv6: string | null }

/**
 * What a public domain currently resolves to, per address family. A public
 * domain is DualStack: the operator points an `A` record at the gateway's WAN
 * IPv4 and an `AAAA` at the box's IPv6 GUA. Either may be absent.
 */
export type QueryDnsResInput = { ipv4?: string | null; ipv6?: string | null }

/**
 * Contiguous port-range binding (e.g. WebRTC/STUN/TURN RTP ranges).
 *
 * Keyed by `internal_start_port` in [`BindingRanges`]. The range covers
 * `internal_start_port..(internal_start_port + number_of_ports)` and is
 * forwarded through a single iptables rule per protocol per gateway,
 * preserving the destination port number.
 */
export type RangeBindInfo = {
  enabled: boolean
  externalStartPort: number
  numberOfPorts: number
  /**
   * Reachable addresses for this range (LAN IPv4 / WAN IPv4 / mDNS /
   * domains) with per-address enabled/disabled overrides — the same model as
   * a single-port binding, but IPv4-only and non-SSL. COMPUTED by
   * `update_addresses`; every entry uses `external_start_port` as its
   * representative port. Public IPs are disabled by default (WAN is opt-in);
   * LAN/mDNS/domains are enabled by default.
   */
  addresses: DerivedAddressInfo
  /**
   * The single restricted `api` interface exported from this range, if any
   * (`RangeOrigin.export`). Preserved across idempotent re-binds.
   */
  interface: RangeServiceInterface | null
}

/**
 * Contiguous port-range binding (e.g. WebRTC/STUN/TURN RTP ranges).
 *
 * Keyed by `internal_start_port` in [`BindingRanges`]. The range covers
 * `internal_start_port..(internal_start_port + number_of_ports)` and is
 * forwarded through a single iptables rule per protocol per gateway,
 * preserving the destination port number.
 */
export type RangeBindInfoInput = {
  enabled: boolean
  externalStartPort: number
  numberOfPorts: number
  /**
   * Reachable addresses for this range (LAN IPv4 / WAN IPv4 / mDNS /
   * domains) with per-address enabled/disabled overrides — the same model as
   * a single-port binding, but IPv4-only and non-SSL. COMPUTED by
   * `update_addresses`; every entry uses `external_start_port` as its
   * representative port. Public IPs are disabled by default (WAN is opt-in);
   * LAN/mDNS/domains are enabled by default.
   */
  addresses?: DerivedAddressInfoInput
  /**
   * The single restricted `api` interface exported from this range, if any
   * (`RangeOrigin.export`). Preserved across idempotent re-binds.
   */
  interface?: RangeServiceInterfaceInput | null
}

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
export type RangeServiceInterface = {
  id: ServiceInterfaceId
  name: string
  description: string
  scheme: string | null
}

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
export type RangeServiceInterfaceInput = {
  id: ServiceInterfaceIdInput
  name: string
  description: string
  scheme?: string | null
}
export type RebuildParams = { id: PackageId }
export type RebuildParamsInput = { id: PackageIdInput }

/**
 * A migration or backup recovery source with a caller-selected backup password representation.
 */
export type RecoverySource<Password> =
  | ({ type: 'migrate' } & { guid: string })
  | ({ type: 'backup' } & {
      target: BackupTargetFS
      password: Password
      serverId: string
    })

/**
 * A migration or backup recovery source with a caller-selected backup password representation.
 */
export type RecoverySourceInput<Password> =
  | ({ type: 'migrate' } & { guid: string })
  | ({ type: 'backup' } & {
      target: BackupTargetFSInput
      password: Password
      serverId: string
    })

/**
 * A registry asset with a caller-selected commitment representation.
 */
export type RegistryAsset<Commitment> = {
  publishedAt: string
  urls: string[]
  commitment: Commitment
  signatures: { [key: string]: AnySignature }
}

/**
 * A registry asset with a caller-selected commitment representation.
 */
export type RegistryAssetInput<Commitment> = {
  publishedAt: string
  urls: string[]
  commitment: Commitment
  signatures: { [key: string]: AnySignatureInput }
}
export type RegistryInfo = {
  name: string | null
  icon: DataUrl | null
  /**
   * Markdown, shown above the registry's services.
   */
  description: LocaleString | null
  categories: { [key: string]: Category }
}
export type RegistryInfoInput = {
  name?: string | null
  icon?: DataUrlInput | null
  /**
   * Markdown, shown above the registry's services.
   */
  description?: LocaleStringInput | null
  categories: { [key: string]: CategoryInput }
}
export type RemoveAcmeParams = { provider: AcmeProvider }
export type RemoveAcmeParamsInput = { provider: AcmeProviderInput }
export type RemoveAdminParams = { signer: Guid }
export type RemoveAdminParamsInput = { signer: GuidInput }
export type RemoveAssetParams = { version: string; platform: string }
export type RemoveAssetParamsInput = { version: string; platform: string }
export type RemoveCategoryParams = { id: string }
export type RemoveCategoryParamsInput = { id: string }
export type RemoveDomainParams = { fqdn: string }
export type RemoveDomainParamsInput = { fqdn: string }
export type RemoveMirrorParams = {
  id: PackageId
  version: Version
  url: string
}
export type RemoveMirrorParamsInput = {
  id: PackageIdInput
  version: VersionInput
  url: string
}
export type RemovePackageFromCategoryParams = { id: string; package: PackageId }
export type RemovePackageFromCategoryParamsInput = {
  id: string
  package: PackageIdInput
}
export type RemovePackageParams = {
  id: PackageId
  version: Version | null
  sighash: string | null
  force: boolean
}
export type RemovePackageParamsInput = {
  id: PackageIdInput
  version?: VersionInput | null
  sighash?: string | null
  force?: boolean
}
export type RemovePackageSignerParams = { id: PackageId; signer: Guid }
export type RemovePackageSignerParamsInput = {
  id: PackageIdInput
  signer: GuidInput
}
export type RemoveSignerParams = { id: Guid }
export type RemoveSignerParamsInput = { id: GuidInput }
export type RemoveTunnelParams = { id: GatewayId }
export type RemoveTunnelParamsInput = { id: GatewayIdInput }
export type RemoveVersionParams = { version: string }
export type RemoveVersionParamsInput = { version: string }
export type RenameGatewayParams = { id: GatewayId; name: string }
export type RenameGatewayParamsInput = { id: GatewayIdInput; name: string }
export type ReplayId = string
export type ReplayIdInput = string
export type RequestCommitment = {
  timestamp: number
  nonce: number
  size: number
  blake3: string
}
export type RequestCommitmentInput = {
  timestamp: number
  nonce: number
  size: number
  blake3: string
}
export type ResetPasswordParams = { newPassword: PasswordType | null }
export type ResetPasswordParamsInput = {
  newPassword?: PasswordTypeInput | null
}
export type RestartReason = 'mdns' | 'language' | 'kiosk' | 'update'
export type RestartReasonInput = 'mdns' | 'language' | 'kiosk' | 'update'
export type RestorePackageParams = {
  targetId: BackupTargetId
  password: string
  ids: PackageId[]
  serverId: string | null
}
export type RestorePackageParamsInput = {
  targetId: BackupTargetIdInput
  password: string
  ids: PackageIdInput[]
  serverId?: string | null
}
export type RetireBindingParams = { id: HostId; internalPort: number }
export type RetireBindingParamsInput = { id: HostIdInput; internalPort: number }
export type RetireHostParams = { id: HostId }
export type RetireHostParamsInput = { id: HostIdInput }
export type RunActionParams = {
  packageId: PackageId
  eventId: Guid | null
  actionId: ActionId
  input: unknown | null
}
export type RunActionParamsInput = {
  packageId: PackageIdInput
  eventId?: GuidInput | null
  actionId: ActionIdInput
  input?: unknown | null
}
export type Security = { ssl: boolean }
export type SecurityInput = { ssl: boolean }
export type ServerBackupReport = { attempted: boolean; error: string | null }
export type ServerBackupReportInput = {
  attempted: boolean
  error?: string | null
}
export type ServerHostname = string
export type ServerHostnameInput = string
export type ServerInfo = {
  id: string
  hostname: string
  version: string
  packageVersionCompat: string
  postInitMigrationTodos: Record<string, unknown>
  latestMigrationRevision: number
  lastBackup: string | null
  network: NetworkInfo
  statusInfo: ServerStatus
  unreadNotificationCount: number
  pubkey: string
  caFingerprint: string
  ntpSynced: boolean
  zram: boolean
  governor: Governor | null
  epp: Epp | null
  smtp: SmtpValue | null
  echoipUrls: string[]
  ram: number
  devices: LshwDevice[]
  kiosk: boolean | null
  language: string | null
  keyboard: KeyboardOptions | null
}
export type ServerInfoInput = {
  id: string
  hostname: string
  version: string
  packageVersionCompat: string
  postInitMigrationTodos: Record<string, unknown>
  latestMigrationRevision?: number
  lastBackup?: string | null
  network: NetworkInfoInput
  statusInfo?: ServerStatusInput
  unreadNotificationCount: number
  pubkey: string
  caFingerprint: string
  ntpSynced?: boolean
  zram?: boolean
  governor?: GovernorInput | null
  epp?: EppInput | null
  smtp?: SmtpValueInput | null
  echoipUrls?: string[]
  ram: number
  devices: LshwDeviceInput[]
  kiosk?: boolean | null
  language?: string | null
  keyboard?: KeyboardOptionsInput | null
}
export type ServerSpecs = { cpu: string; disk: string; memory: string }
export type ServerSpecsInput = { cpu: string; disk: string; memory: string }
export type ServerStatus = {
  backupProgress: FullProgress | null
  updateProgress: FullProgress | null
  shuttingDown: boolean
  restarting: boolean
  restart: RestartReason | null
}
export type ServerStatusInput = {
  backupProgress?: FullProgressInput | null
  updateProgress?: FullProgressInput | null
  shuttingDown?: boolean
  restarting?: boolean
  restart?: RestartReasonInput | null
}
export type ServiceDependencyMetadata = { title: LocaleString }
export type ServiceDependencyMetadataInput = { title: LocaleStringInput }
export type ServiceInterface = {
  id: ServiceInterfaceId
  name: string
  description: string
  masked: boolean
  addressInfo: AddressInfo
  type: ServiceInterfaceType
  /**
   * The interface address Open UI should prefer.
   */
  preferredLauncherAddress: string | null
}
export type ServiceInterfaceId = string
export type ServiceInterfaceIdInput = string
export type ServiceInterfaceInput = {
  id: ServiceInterfaceIdInput
  name: string
  description: string
  masked: boolean
  addressInfo: AddressInfoInput
  type: ServiceInterfaceTypeInput
  /**
   * The interface address Open UI should prefer.
   */
  preferredLauncherAddress?: string | null
}
export type ServiceInterfaceType = 'ui' | 'p2p' | 'api'
export type ServiceInterfaceTypeInput = 'ui' | 'p2p' | 'api'
export type Session = {
  /**
   * A friendly name for the key, if one was assigned at enrollment (e.g.
   * tunnel device keys). UI-enrolled keys are unnamed.
   */
  name: string | null
  loggedIn: string
  lastActive: string
  userAgent: string | null
}
export type SessionInput = {
  /**
   * A friendly name for the key, if one was assigned at enrollment (e.g.
   * tunnel device keys). UI-enrolled keys are unnamed.
   */
  name?: string | null
  loggedIn?: string
  lastActive?: string
  userAgent?: string | null
}
export type SessionList = { current: string | null; sessions: AuthKeys }
export type SessionListInput = {
  current?: string | null
  sessions: AuthKeysInput
}
export type SetBackupProgress = { progress: Progress }
export type SetBackupProgressInput = { progress: ProgressInput }
export type SetCountryParams = { country: string }
export type SetCountryParamsInput = { country: string }
export type SetDataVersionParams = { version: string | null }
export type SetDataVersionParamsInput = { version?: string | null }
export type SetDefaultOutboundParams = { gateway: GatewayId | null }
export type SetDefaultOutboundParamsInput = { gateway?: GatewayIdInput | null }
export type SetDependenciesParams = { dependencies: DependencyRequirement[] }
export type SetDependenciesParamsInput = {
  dependencies: DependencyRequirementInput[]
}
export type SetDescriptionParams = { description: LocaleString | null }
export type SetDescriptionParamsInput = {
  description: LocaleStringInput | null
}
export type SetGatewaySecureParams = {
  gateway: GatewayId
  secure: boolean | null
}
export type SetGatewaySecureParamsInput = {
  gateway: GatewayIdInput
  secure?: boolean | null
}
export type SetHealth = { id: HealthCheckId } & NamedHealthCheckResult
export type SetHealthInput = {
  id: HealthCheckIdInput
} & NamedHealthCheckResultInput
export type SetIconParams = { icon: DataUrl | null }
export type SetIconParamsInput = { icon: DataUrlInput | null }
export type SetInitProgress = { progress: Progress }
export type SetInitProgressInput = { progress: ProgressInput }
export type SetLanguageParams = { language: string }
export type SetLanguageParamsInput = { language: string }
export type SetMainStatus = { status: SetMainStatusStatus }
export type SetMainStatusInput = { status: SetMainStatusStatusInput }
export type SetMainStatusStatus = 'running' | 'stopped'
export type SetMainStatusStatusInput = 'running' | 'stopped'
export type SetNameParams = { name: string }
export type SetNameParamsInput = { name: string }
export type SetOutboundGatewayParams = {
  package: PackageId
  gateway: GatewayId | null
}
export type SetOutboundGatewayParamsInput = {
  package: PackageIdInput
  gateway?: GatewayIdInput | null
}
export type SetServerHostnameParams = {
  /**
   * The server's `.local` hostname: up to 32 lowercase letters, numbers, and
   * hyphens, not starting or ending with a hyphen
   */
  hostname: string
}
export type SetServerHostnameParamsInput = {
  /**
   * The server's `.local` hostname: up to 32 lowercase letters, numbers, and
   * hyphens, not starting or ending with a hyphen
   */
  hostname: string
}
export type SetStaticDnsParams = { servers: string[] | null }
export type SetStaticDnsParamsInput = { servers?: string[] | null }
export type SetWifiEnabledParams = { enabled: boolean }
export type SetWifiEnabledParamsInput = { enabled: boolean }
export type SetupExecuteParams = {
  guid: string
  password: EncryptedWire | null
  recoverySource: RecoverySource<EncryptedWire> | null
  kiosk: boolean
  hostname: string | null
}
export type SetupExecuteParamsInput = {
  guid: string
  password?: EncryptedWireInput | null
  recoverySource?: RecoverySourceInput<EncryptedWireInput> | null
  kiosk: boolean
  hostname?: string | null
}
export type SetupInfo = {
  guid: string | null
  attach: boolean
  mokEnrolled: boolean
  /**
   * The whole disk the OS is installed on, recorded by whatever installed it
   * (os_install, or init_resize for a pre-installed image). When the device
   * is pre-installed the wizard fixes this as the OS drive and asks only for
   * a data drive.
   */
  osDrive: string | null
}
export type SetupInfoInput = {
  guid?: string | null
  attach: boolean
  mokEnrolled: boolean
  /**
   * The whole disk the OS is installed on, recorded by whatever installed it
   * (os_install, or init_resize for a pre-installed image). When the device
   * is pre-installed the wizard fixes this as the OS drive and asks only for
   * a data drive.
   */
  osDrive?: string | null
}
export type SetupProgress = { progress: FullProgress; guid: Guid }
export type SetupProgressInput = {
  progress: FullProgressInput
  guid: GuidInput
}
export type SetupResult = {
  hostname: string
  rootCa: string
  needsRestart: boolean
}
export type SetupResultInput = {
  hostname: string
  rootCa: string
  needsRestart: boolean
}
export type SetupStatusRes =
  | { status: 'needs-install' }
  | (
      | ({ status: 'incomplete' } & Exclude<SetupInfo, null>)
      | (null extends SetupInfo ? { status: 'incomplete' } : never)
    )
  | (
      | ({ status: 'running' } & Exclude<SetupProgress, null>)
      | (null extends SetupProgress ? { status: 'running' } : never)
    )
  | (
      | ({ status: 'complete' } & Exclude<SetupResult, null>)
      | (null extends SetupResult ? { status: 'complete' } : never)
    )
export type SetupStatusResInput =
  | { status: 'needs-install' }
  | (
      | ({ status: 'incomplete' } & Exclude<SetupInfoInput, null>)
      | (null extends SetupInfoInput ? { status: 'incomplete' } : never)
    )
  | (
      | ({ status: 'running' } & Exclude<SetupProgressInput, null>)
      | (null extends SetupProgressInput ? { status: 'running' } : never)
    )
  | (
      | ({ status: 'complete' } & Exclude<SetupResultInput, null>)
      | (null extends SetupResultInput ? { status: 'complete' } : never)
    )
export type ShutdownParams = {
  /**
   * Block until graceful teardown completes (default over the CLI; the
   * frontend omits this and gets an immediate reply). Cleared with
   * `--nowait`. The wait can't outlive the webserver teardown that follows
   * container shutdown, so the connection drops once services are stopped.
   */
  wait: boolean
}
export type ShutdownParamsInput = {
  /**
   * Block until graceful teardown completes (default over the CLI; the
   * frontend omits this and gets an immediate reply). Cleared with
   * `--nowait`. The wait can't outlive the webserver teardown that follows
   * container shutdown, so the connection drops once services are stopped.
   */
  wait?: boolean
}
export type SideloadParams = {}
export type SideloadParamsInput = {}
export type SideloadResponse = { upload: Guid; progress: Guid }
export type SideloadResponseInput = { upload: GuidInput; progress: GuidInput }
export type SignAssetParams = {
  version: string
  platform: string
  signature: AnySignature
}
export type SignAssetParamsInput = {
  version: string
  platform: string
  signature: AnySignatureInput
}

/**
 * So a signal strength is a number between 0-100, I want the null option to be 0 since there is no signal
 */
export type SignalStrength = number

/**
 * So a signal strength is a number between 0-100, I want the null option to be 0 since there is no signal
 */
export type SignalStrengthInput = number
export type SignerInfo = {
  name: string
  contact: ContactInfo[]
  keys: AnyVerifyingKey[]
}
export type SignerInfoInput = {
  name: string
  contact: ContactInfoInput[]
  keys: AnyVerifyingKeyInput[]
}
export type SmtpSecurity = 'starttls' | 'tls'
export type SmtpSecurityInput = 'starttls' | 'tls'
export type SmtpValue = {
  host: string
  port: number
  from: string
  username: string
  password: string | null
  security: SmtpSecurity
}
export type SmtpValueInput = {
  host?: string
  server?: string
  port: number
  from: string
  login?: string
  username?: string
  password?: string | null
  security?: SmtpSecurityInput
} & ({ host: string } | { server: string }) &
  ({ login: string } | { username: string })
export type SshAddParams = { key: SshPubKey }
export type SshAddParamsInput = { key: SshPubKeyInput }
export type SshDeleteParams = { fingerprint: string }
export type SshDeleteParamsInput = { fingerprint: string }
export type SshKeyResponse = {
  alg: string
  fingerprint: string
  hostname: string
  createdAt: string
}
export type SshKeyResponseInput = {
  alg: string
  fingerprint: string
  hostname: string
  createdAt: string
}
export type SshPubKey = string
export type SshPubKeyInput = string

/**
 * Ssid are the names of the wifis, usually human readable.
 */
export type Ssid = string

/**
 * Ssid are the names of the wifis, usually human readable.
 */
export type SsidInput = string

/**
 * The public view of a backup found on a target: enough to identify it, and none of
 * [`BackupUnencryptedMetadata`]'s key material.
 */
export type StartOsRecoveryInfo = {
  hostname: ServerHostname
  version: string
  timestamp: string
}

/**
 * The public view of a backup found on a target: enough to identify it, and none of
 * [`BackupUnencryptedMetadata`]'s key material.
 */
export type StartOsRecoveryInfoInput = {
  hostname: ServerHostnameInput
  version: string
  timestamp: string
}
export type StartStop = 'start' | 'stop'
export type StartStopInput = 'start' | 'stop'
export type StatusInfo = {
  health: { [key: string]: NamedHealthCheckResult }
  error: ErrorData | null
  started: string | null
  desired: DesiredStatus
}
export type StatusInfoInput = {
  health: { [key: string]: NamedHealthCheckResultInput }
  error?: ErrorDataInput | null
  started?: string | null
  desired: DesiredStatusInput
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
export type TaskEntryInput = { task: TaskParams; active: boolean }
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
export type TestSmtpParams = {
  host: string
  port: number
  from: string
  to: string
  username: string
  password: string
  security: SmtpSecurity
}
export type TestSmtpParamsInput = {
  host: string
  port: number
  from: string
  to: string
  username: string
  password: string
  security?: SmtpSecurityInput
}
export type TimeInfo = { now: string; uptime: number }
export type TimeInfoInput = { now: string; uptime: number }
export type UmountParams = { targetId: BackupTargetId | null }
export type UmountParamsInput = { targetId?: BackupTargetIdInput | null }
export type UninstallParams = { id: PackageId; soft: boolean; force: boolean }
export type UninstallParamsInput = {
  id: PackageIdInput
  soft?: boolean
  force?: boolean
}
export type UnsetGatewaySecureParams = { gateway: GatewayId }
export type UnsetGatewaySecureParamsInput = { gateway: GatewayIdInput }
export type UpdateSystemParams = {
  registry: string
  targetVersion: string | null
  progress: boolean
}
export type UpdateSystemParamsInput = {
  registry: string
  targetVersion?: string | null
  progress?: boolean
}
export type UpdateSystemRes = { target: string | null; progress: string | null }
export type UpdateSystemResInput = {
  target?: string | null
  progress?: string | null
}
export type UpdateTunnelParams = { id: GatewayId; config: string }
export type UpdateTunnelParamsInput = { id: GatewayIdInput; config: string }
export type UpdatingState = {
  manifest: Manifest
  s9pk: string
  installingInfo: InstallingInfo
}
export type UpdatingStateInput = {
  manifest: ManifestInput
  s9pk: string
  installingInfo: InstallingInfoInput
}

/**
 * How the OS reverse proxy validates the container's TLS certificate when it
 * rewraps SSL (`add_ssl` set AND `secure.ssl == true`, so the OS terminates
 * the client's TLS and initiates a fresh TLS connection to the container).
 * Absent (`None`) means validate against the StartOS root CA — the default.
 */
export type UpstreamCertValidation =
  /**
   * Do not validate the container's certificate at all. Use when the
   * container serves a self-signed cert on the trusted internal bridge.
   */
  | 'disable'
  | {
      /**
       * Validate against this PEM-encoded certificate (or chain) instead of the
       * StartOS root CA.
       */
      certificate: string
    }

/**
 * How the OS reverse proxy validates the container's TLS certificate when it
 * rewraps SSL (`add_ssl` set AND `secure.ssl == true`, so the OS terminates
 * the client's TLS and initiates a fresh TLS connection to the container).
 * Absent (`None`) means validate against the StartOS root CA — the default.
 */
export type UpstreamCertValidationInput =
  /**
   * Do not validate the container's certificate at all. Use when the
   * container serves a self-signed cert on the trusted internal bridge.
   */
  | 'disable'
  | {
      /**
       * Validate against this PEM-encoded certificate (or chain) instead of the
       * StartOS root CA.
       */
      certificate: string
    }
export type UrlPluginClearUrlsParams = { except: PluginHostnameInfo[] }
export type UrlPluginClearUrlsParamsInput = {
  except: PluginHostnameInfoInput[]
}
export type UrlPluginExportUrlParams = {
  hostnameInfo: PluginHostnameInfo
  removeAction: ActionId | null
  overflowActions: ActionId[]
}
export type UrlPluginExportUrlParamsInput = {
  hostnameInfo: PluginHostnameInfoInput
  removeAction?: ActionIdInput | null
  overflowActions: ActionIdInput[]
}
export type UrlPluginRegisterParams = { tableAction: ActionId }
export type UrlPluginRegisterParamsInput = { tableAction: ActionIdInput }
export type UrlPluginRegistration = { tableAction: ActionId }
export type UrlPluginRegistrationInput = { tableAction: ActionIdInput }
export type UsersResponse = { uniqueServers: number; totalCheckins: number }
export type UsersResponseInput = {
  uniqueServers: number
  totalCheckins: number
}
export type VerifyCifsParams = {
  hostname: string
  path: string
  username: string
  password: EncryptedWire | null
}
export type VerifyCifsParamsInput = {
  hostname: string
  path: string
  username: string
  password?: EncryptedWireInput | null
}
export type Version = string
export type VersionInput = string
export type VersionSignerParams = { version: string; signer: Guid }
export type VersionSignerParamsInput = { version: string; signer: GuidInput }
export type VolumeId = string
export type VolumeIdInput = string
export type WifiAddParams = { ssid: string; password: string }
export type WifiAddParamsInput = { ssid: string; password: string }
export type WifiInfo = {
  enabled: boolean
  interface: GatewayId | null
  ssids: string[]
  selected: string | null
  lastRegion: string | null
}
export type WifiInfoInput = {
  enabled: boolean
  interface?: GatewayIdInput | null
  ssids: string[]
  selected?: string | null
  lastRegion?: string | null
}
export type WifiListInfo = {
  ssids: { [key: string]: SignalStrength }
  connected: Ssid | null
  country: string | null
  ethernet: boolean
  availableWifi: WifiListOut[]
}
export type WifiListInfoInput = {
  ssids: { [key: string]: SignalStrengthInput }
  connected?: SsidInput | null
  country?: string | null
  ethernet: boolean
  availableWifi: WifiListOutInput[]
}
export type WifiListOut = {
  ssid: Ssid
  strength: SignalStrength
  security: string[]
}
export type WifiListOutInput = {
  ssid: SsidInput
  strength: SignalStrengthInput
  security: string[]
}
export type WifiSsidParams = { ssid: string }
export type WifiSsidParamsInput = { ssid: string }
