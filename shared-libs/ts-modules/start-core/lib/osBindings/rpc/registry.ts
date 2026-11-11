import type { z } from 'zod'
import type * as schemas from './registry.schemas'
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

/**
 * JSON Patch 'add' operation representation
 */
export type AddOperation = z.output<typeof schemas.AddOperation>
export type AnySignature = z.output<typeof schemas.AnySignature>
export type AnySignatureInput = z.input<typeof schemas.AnySignature>
export type AnyVerifyingKey = z.output<typeof schemas.AnyVerifyingKey>
export type AnyVerifyingKeyInput = z.input<typeof schemas.AnyVerifyingKey>
export type ApplyParamsInput = z.input<typeof schemas.ApplyParams>
export type Blake3Commitment = z.output<typeof schemas.Blake3Commitment>
export type Blake3CommitmentInput = z.input<typeof schemas.Blake3Commitment>
export type ContactInfo = z.output<typeof schemas.ContactInfo>
export type ContactInfoInput = z.input<typeof schemas.ContactInfo>

/**
 * JSON Patch 'copy' operation representation
 */
export type CopyOperation = z.output<typeof schemas.CopyOperation>
export type CountEntry = z.output<typeof schemas.CountEntry>
export type DataUrl = z.output<typeof schemas.DataUrl>
export type DataUrlInput = z.input<typeof schemas.DataUrl>
export type DeviceFilter = z.output<typeof schemas.DeviceFilter>
export type Dump = z.output<typeof schemas.Dump>
export type DumpParamsInput = z.input<typeof schemas.DumpParams>
export type GetDownloadsParamsInput = z.input<typeof schemas.GetDownloadsParams>
export type GetOsAssetParamsInput = z.input<typeof schemas.GetOsAssetParams>
export type GetOsVersionParamsInput = z.input<typeof schemas.GetOsVersionParams>
export type GetUsersParamsInput = z.input<typeof schemas.GetUsersParams>
export type GitHash = z.output<typeof schemas.GitHash>
export type GitHashInput = z.input<typeof schemas.GitHash>

/**
 * An original public-key export and its self-certified identity claims.
 */
export type GpgPublicKeyExport = z.output<typeof schemas.GpgPublicKeyExport>

/**
 * An original public-key export and its self-certified identity claims.
 */
export type GpgPublicKeyExportInput = z.input<typeof schemas.GpgPublicKeyExport>
export type Guid = z.output<typeof schemas.Guid>
export type GuidInput = z.input<typeof schemas.Guid>
export type HealthCheckId = z.output<typeof schemas.HealthCheckId>
export type IoFormatInput = z.input<typeof schemas.IoFormat>
export type ListVersionSignersParamsInput = z.input<
  typeof schemas.ListVersionSignersParams
>
export type LocaleString = z.output<typeof schemas.LocaleString>
export type LocaleStringInput = z.input<typeof schemas.LocaleString>
export type MerkleArchiveCommitment = z.output<
  typeof schemas.MerkleArchiveCommitment
>
export type MerkleArchiveCommitmentInput = z.input<
  typeof schemas.MerkleArchiveCommitment
>

/**
 * JSON Patch 'move' operation representation
 */
export type MoveOperation = z.output<typeof schemas.MoveOperation>
export type PackageDetailLevelInput = z.input<typeof schemas.PackageDetailLevel>
export type PackageIdInput = z.input<typeof schemas.PackageId>
export type PackageVersionCount = z.output<typeof schemas.PackageVersionCount>
export type PluginId = z.output<typeof schemas.PluginId>
export type PreDownloadAlertWhen = z.output<typeof schemas.PreDownloadAlertWhen>
export type ReaderParamsInput = z.input<typeof schemas.ReaderParams>

/**
 * A registry asset with a caller-selected commitment representation.
 */
export type RegistryAsset<Commitment> = z.output<
  ReturnType<typeof schemas.RegistryAsset<z.ZodType<Commitment, Commitment>>>
>
export type RemoveAssetParamsInput = z.input<typeof schemas.RemoveAssetParams>
export type RemoveCategoryParamsInput = z.input<
  typeof schemas.RemoveCategoryParams
>

/**
 * JSON Patch 'remove' operation representation
 */
export type RemoveOperation = z.output<typeof schemas.RemoveOperation>
export type RemoveVersionParamsInput = z.input<
  typeof schemas.RemoveVersionParams
>

/**
 * JSON Patch 'replace' operation representation
 */
export type ReplaceOperation = z.output<typeof schemas.ReplaceOperation>
export type SetNameParamsInput = z.input<typeof schemas.SetNameParams>

/**
 * JSON Patch 'test' operation representation
 */
export type TestOperation = z.output<typeof schemas.TestOperation>
export type UsersResponse = z.output<typeof schemas.UsersResponse>
export type Version = z.output<typeof schemas.Version>
export type VersionInput = z.input<typeof schemas.Version>
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
export type AddSignerParamsInput = z.input<typeof schemas.AddSignerParams>
export type AddVersionParamsInput = z.input<typeof schemas.AddVersionParams>
export type ApplyWithPathParamsInput = z.input<
  typeof schemas.ApplyWithPathParams
>
export type Category = z.output<typeof schemas.Category>
export type CurrentDependencyKind = z.output<
  typeof schemas.CurrentDependencyKind
>
export type Description = z.output<typeof schemas.Description>
export type DownloadsResponse = z.output<typeof schemas.DownloadsResponse>
export type EditSignerParamsInput = z.input<typeof schemas.EditSignerParams>
export type GetPackageAccessParamsInput = z.input<
  typeof schemas.GetPackageAccessParams
>
export type GetPackageParamsInput = z.input<typeof schemas.GetPackageParams>
export type HardwareRequirements = z.output<typeof schemas.HardwareRequirements>
export type ListPackageSignersParamsInput = z.input<
  typeof schemas.ListPackageSignersParams
>
export type MetricsSummary = z.output<typeof schemas.MetricsSummary>
export type OsVersionInfo = z.output<typeof schemas.OsVersionInfo>
export type PackageAccessControl = z.output<typeof schemas.PackageAccessControl>

/**
 * Inline for StartOS before 0.4.0.3 and from registries before 1.2.0.
 */
export type PackageIcon = z.output<typeof schemas.PackageIcon>
export type PackageInfoShort = z.output<typeof schemas.PackageInfoShort>
export type PackageReaderParamsInput = z.input<
  typeof schemas.PackageReaderParams
>

/**
 * JSON Patch single patch operation
 */
export type PatchOperation = z.output<typeof schemas.PatchOperation>
export type PreDownloadAlert = z.output<typeof schemas.PreDownloadAlert>
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
export type SetDescriptionParamsInput = z.input<
  typeof schemas.SetDescriptionParams
>
export type SetIconParamsInput = z.input<typeof schemas.SetIconParams>
export type SetPackagePrivateParamsInput = z.input<
  typeof schemas.SetPackagePrivateParams
>
export type SetTrustSignersParamsInput = z.input<
  typeof schemas.SetTrustSignersParams
>
export type SignAssetParamsInput = z.input<typeof schemas.SignAssetParams>
export type SignPackageParamsInput = z.input<typeof schemas.SignPackageParams>
export type SignerInfo = z.output<typeof schemas.SignerInfo>
export type SubscribeIndexParamsInput = z.input<
  typeof schemas.SubscribeIndexParams
>
export type SubscribeRes = z.output<typeof schemas.SubscribeRes>
export type VersionSignerParamsInput = z.input<
  typeof schemas.VersionSignerParams
>
export type DependencyMetadata = z.output<typeof schemas.DependencyMetadata>
export type OsVersionInfoMap = z.output<typeof schemas.OsVersionInfoMap>
export type PackageAccess = z.output<typeof schemas.PackageAccess>
export type PackageMetadata = z.output<typeof schemas.PackageMetadata>

/**
 * Representation of JSON Patch (list of patch operations)
 */
export type Patch = z.output<typeof schemas.Patch>
export type RegistryInfo = z.output<typeof schemas.RegistryInfo>

/**
 * Opted-in rpcstream requests receive the dump directly; other callers receive a GUID.
 */
export type SubscribeResult = z.output<typeof schemas.SubscribeResult>

/**
 * Database-generated patches contain only add, remove and replace operations.
 */
export type DiffPatch = z.output<typeof schemas.DiffPatch>
export type OsIndex = z.output<typeof schemas.OsIndex>
export type PackageVersionInfo = z.output<typeof schemas.PackageVersionInfo>
export type GetPackageResponse = z.output<typeof schemas.GetPackageResponse>
export type GetPackageResponseFull = z.output<
  typeof schemas.GetPackageResponseFull
>
export type PackageInfo = z.output<typeof schemas.PackageInfo>
export type Revision = z.output<typeof schemas.Revision>
export type GetPackageResult = z.output<typeof schemas.GetPackageResult>
export type PackageIndex = z.output<typeof schemas.PackageIndex>
export type FullIndex = z.output<typeof schemas.FullIndex>
export type Api = RpcSchemaTree<typeof schemas.Api>
