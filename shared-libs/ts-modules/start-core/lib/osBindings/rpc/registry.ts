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

export type AddAdminParamsInput = { signer: GuidInput }
export type AddAssetParamsInput = {
  version: string
  platform: string
  url: string
  signature: AnySignatureInput
  commitment: Blake3CommitmentInput
}
export type AddCategoryParamsInput = { id: string; name: LocaleStringInput }
export type AddMirrorParamsInput = {
  url: string
  commitment: MerkleArchiveCommitmentInput
  signature: AnySignatureInput
}
export type AddPackageParamsInput = {
  urls: string[]
  commitment: MerkleArchiveCommitmentInput
  signature: AnySignatureInput
}
export type AddPackageSignerParamsInput = {
  id: PackageIdInput
  signer: GuidInput
  versions?: string | null
  merge: boolean
}
export type AddPackageToCategoryParamsInput = {
  id: string
  package: PackageIdInput
}
export type AddVersionParamsInput = {
  version: string
  headline: string
  releaseNotes: string
  sourceVersion: string
}
export type AnySignature = string
export type AnySignatureInput = string
export type AnyVerifyingKey = string
export type AnyVerifyingKeyInput = string
export type Api = {
  _PARAMS: {}
  _CHILDREN: {
    admin: {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: AddAdminParamsInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: { [key: string]: SignerInfo }
        }
        remove: { _PARAMS: RemoveAdminParamsInput; _RETURN: null }
        signer: {
          _PARAMS: {}
          _CHILDREN: {
            add: { _PARAMS: SignerInfoInput; _RETURN: Guid }
            edit: { _PARAMS: EditSignerParamsInput; _RETURN: null }
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: { [key: string]: SignerInfo }
            }
            remove: { _PARAMS: RemoveSignerParamsInput; _RETURN: null }
          }
        }
      }
    }
    db: {
      _PARAMS: {}
      _CHILDREN: {
        apply: { _PARAMS: ApplyWithPathParamsInput; _RETURN: null }
        dump: { _PARAMS: DumpParamsInput; _RETURN: Dump }
        subscribe: { _PARAMS: DumpParamsInput; _RETURN: SubscribeRes }
      }
    }
    index: {
      _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
      _RETURN: FullIndex
    }
    info: {
      _PARAMS: { format?: IoFormatInput | null } & {}
      _RETURN: RegistryInfo
      _CHILDREN: {
        'set-description': { _PARAMS: SetDescriptionParamsInput; _RETURN: null }
        'set-icon': { _PARAMS: SetIconParamsInput; _RETURN: null }
        'set-name': { _PARAMS: SetNameParamsInput; _RETURN: null }
      }
    }
    metrics: {
      _PARAMS: {}
      _CHILDREN: {
        downloads: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            GetDownloadsParamsInput
          _RETURN: DownloadsResponse
        }
        summary: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: MetricsSummary
        }
        users: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            GetUsersParamsInput
          _RETURN: UsersResponse
        }
      }
    }
    os: {
      _PARAMS: {}
      _CHILDREN: {
        asset: {
          _PARAMS: {}
          _CHILDREN: {
            add: {
              _PARAMS: {}
              _CHILDREN: {
                img: { _PARAMS: AddAssetParamsInput; _RETURN: null }
                iso: { _PARAMS: AddAssetParamsInput; _RETURN: null }
                squashfs: { _PARAMS: AddAssetParamsInput; _RETURN: null }
              }
            }
            get: {
              _PARAMS: {}
              _CHILDREN: {
                img: {
                  _PARAMS: GetOsAssetParamsInput
                  _RETURN: RegistryAsset<Blake3Commitment>
                }
                iso: {
                  _PARAMS: GetOsAssetParamsInput
                  _RETURN: RegistryAsset<Blake3Commitment>
                }
                squashfs: {
                  _PARAMS: GetOsAssetParamsInput
                  _RETURN: RegistryAsset<Blake3Commitment>
                }
              }
            }
            remove: {
              _PARAMS: {}
              _CHILDREN: {
                img: { _PARAMS: RemoveAssetParamsInput; _RETURN: boolean }
                iso: { _PARAMS: RemoveAssetParamsInput; _RETURN: boolean }
                squashfs: { _PARAMS: RemoveAssetParamsInput; _RETURN: boolean }
              }
            }
            sign: {
              _PARAMS: {}
              _CHILDREN: {
                img: { _PARAMS: SignAssetParamsInput; _RETURN: null }
                iso: { _PARAMS: SignAssetParamsInput; _RETURN: null }
                squashfs: { _PARAMS: SignAssetParamsInput; _RETURN: null }
              }
            }
          }
        }
        index: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: OsIndex
        }
        version: {
          _PARAMS: {}
          _CHILDREN: {
            add: { _PARAMS: AddVersionParamsInput; _RETURN: null }
            get: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                GetOsVersionParamsInput
              _RETURN: OsVersionInfoMap
            }
            remove: { _PARAMS: RemoveVersionParamsInput; _RETURN: null }
            signer: {
              _PARAMS: {}
              _CHILDREN: {
                add: { _PARAMS: VersionSignerParamsInput; _RETURN: null }
                list: {
                  _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                    ListVersionSignersParamsInput
                  _RETURN: { [key: string]: SignerInfo }
                }
                remove: { _PARAMS: VersionSignerParamsInput; _RETURN: null }
              }
            }
          }
        }
      }
    }
    package: {
      _PARAMS: {}
      _CHILDREN: {
        add: { _PARAMS: AddPackageParamsInput; _RETURN: null }
        'add-mirror': { _PARAMS: AddMirrorParamsInput; _RETURN: null }
        category: {
          _PARAMS: {}
          _CHILDREN: {
            add: { _PARAMS: AddCategoryParamsInput; _RETURN: null }
            'add-package': {
              _PARAMS: AddPackageToCategoryParamsInput
              _RETURN: null
            }
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
              _RETURN: { [key: string]: Category }
            }
            remove: { _PARAMS: RemoveCategoryParamsInput; _RETURN: null }
            'remove-package': {
              _PARAMS: RemovePackageFromCategoryParamsInput
              _RETURN: null
            }
          }
        }
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            GetPackageParamsInput
          _RETURN: GetPackageResult
        }
        index: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: PackageIndex
        }
        remove: { _PARAMS: RemovePackageParamsInput; _RETURN: boolean }
        'remove-mirror': { _PARAMS: RemoveMirrorParamsInput; _RETURN: null }
        signer: {
          _PARAMS: {}
          _CHILDREN: {
            add: { _PARAMS: AddPackageSignerParamsInput; _RETURN: null }
            list: {
              _PARAMS: ({ format?: IoFormatInput | null } & {}) &
                ListPackageSignersParamsInput
              _RETURN: { [key: string]: [SignerInfo, string] }
            }
            remove: { _PARAMS: RemovePackageSignerParamsInput; _RETURN: null }
          }
        }
      }
    }
  }
}
export type ApplyParamsInput = { expr: string }
export type ApplyWithPathParamsInput = {
  path?: string | null
} & ApplyParamsInput
export type Blake3Commitment = { hash: string; size: number }
export type Blake3CommitmentInput = { hash: string; size: number }
export type Category = { name: LocaleString }
export type ContactInfo =
  | { email: string }
  | { matrix: string }
  | { website: string }
export type ContactInfoInput =
  | { email: string }
  | { matrix: string }
  | { website: string }
export type CountEntry = { label: string; count: number }
export type CurrentDependencyKind =
  | { kind: 'exists' }
  | ({ kind: 'running' } & { healthChecks: string[] })
export type DataUrl = string
export type DataUrlInput = string
export type DependencyMetadata = {
  title: LocaleString | null
  icon: DataUrl | null
  description: LocaleString | null
  optional: boolean
  versionRange: string | null
} & Partial<Exclude<CurrentDependencyKind | null, null>>
export type Description = { short: LocaleString; long: LocaleString }
export type DeviceFilter = {
  description: string
  class: 'processor' | 'display'
  product: string | null
  vendor: string | null
  capabilities: string[] | null
  driver: string | null
}
export type DownloadsResponse = {
  totalRequests: number
  byPackage: CountEntry[]
  byPackageVersion: PackageVersionCount[]
}
export type Dump = { id: number; value: unknown }
export type DumpParamsInput = { pointer?: string | null }
export type EditSignerParamsInput = {
  id: GuidInput
  setName?: string | null
  addContact: ContactInfoInput[]
  addKey: AnyVerifyingKeyInput[]
  removeContact: ContactInfoInput[]
  removeKey: AnyVerifyingKeyInput[]
}
export type FullIndex = {
  name: string | null
  icon: DataUrl | null
  description: LocaleString | null
  package: PackageIndex
  os: OsIndex
  signers: { [key: string]: SignerInfo }
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
export type GetOsAssetParamsInput = { version: string; platform: string }
export type GetOsVersionParamsInput = {
  sourceVersion?: string | null
  targetVersion?: string | null
  serverId?: string | null
  platform?: string | null
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
export type GetPackageResult =
  | GetPackageResponseFull
  | GetPackageResponse
  | { [key: string]: GetPackageResponseFull }
  | { [key: string]: GetPackageResponse }
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
export type GitHash = string
export type Guid = string
export type GuidInput = string
export type HardwareRequirements = {
  device: DeviceFilter[]
  ram: number | null
  arch: string[] | null
}
export type IoFormatInput =
  | 'json'
  | 'json-pretty'
  | 'yaml'
  | 'cbor'
  | 'toml'
  | 'toml-pretty'
export type ListPackageSignersParamsInput = { id: PackageIdInput }
export type ListVersionSignersParamsInput = { version: string }
export type LocaleString = string | Record<string, string>
export type LocaleStringInput = string | Record<string, string>
export type MerkleArchiveCommitment = {
  rootSighash: string
  rootMaxsize: number
}
export type MerkleArchiveCommitmentInput = {
  rootSighash: string
  rootMaxsize: number
}
export type MetricsSummary = {
  totalCheckins: number
  uniqueServers: number
  totalPackageRequests: number
  byArch: CountEntry[]
  byOsVersion: CountEntry[]
}
export type OsIndex = { versions: OsVersionInfoMap }
export type OsVersionInfo = {
  headline: string
  releaseNotes: string
  sourceVersion: string
  authorized: Guid[]
  iso: { [key: string]: RegistryAsset<Blake3Commitment> }
  squashfs: { [key: string]: RegistryAsset<Blake3Commitment> }
  img: { [key: string]: RegistryAsset<Blake3Commitment> }
}
export type OsVersionInfoMap = { [key: string]: OsVersionInfo }
export type PackageDetailLevelInput = 'none' | 'short' | 'full'
export type PackageIdInput = string
export type PackageIndex = {
  categories: { [key: string]: Category }
  packages: { [key: string]: PackageInfo }
}
export type PackageInfo = {
  authorized: { [key: string]: string }
  versions: { [key: string]: PackageVersionInfo }
  categories: string[]
}
export type PackageInfoShort = { releaseNotes: LocaleString }
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
export type PackageVersionCount = {
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
export type PluginId = 'url-v0'
export type PreDownloadAlert = {
  message: LocaleString
  when: PreDownloadAlertWhen
}
export type PreDownloadAlertWhen = { sourceVersion: string }

/**
 * A registry asset with a caller-selected commitment representation.
 */
export type RegistryAsset<Commitment> = {
  publishedAt: string
  urls: string[]
  commitment: Commitment
  signatures: { [key: string]: AnySignature }
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
export type RemoveAdminParamsInput = { signer: GuidInput }
export type RemoveAssetParamsInput = { version: string; platform: string }
export type RemoveCategoryParamsInput = { id: string }
export type RemoveMirrorParamsInput = {
  id: PackageIdInput
  version: VersionInput
  url: string
}
export type RemovePackageFromCategoryParamsInput = {
  id: string
  package: PackageIdInput
}
export type RemovePackageParamsInput = {
  id: PackageIdInput
  version?: VersionInput | null
  sighash?: string | null
  force?: boolean
}
export type RemovePackageSignerParamsInput = {
  id: PackageIdInput
  signer: GuidInput
}
export type RemoveSignerParamsInput = { id: GuidInput }
export type RemoveVersionParamsInput = { version: string }
export type SetDescriptionParamsInput = {
  description: LocaleStringInput | null
}
export type SetIconParamsInput = { icon: DataUrlInput | null }
export type SetNameParamsInput = { name: string }
export type SignAssetParamsInput = {
  version: string
  platform: string
  signature: AnySignatureInput
}
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
export type SubscribeRes = { dump: Dump; guid: Guid }
export type UsersResponse = { uniqueServers: number; totalCheckins: number }
export type Version = string
export type VersionInput = string
export type VersionSignerParamsInput = { version: string; signer: GuidInput }
