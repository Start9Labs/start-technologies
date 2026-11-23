import { RPC, T } from '@start9labs/start-core'

type PackageParams = RPC.RpcParamType<RPC.Registry, 'package.get'>
type PackageResult = RPC.RpcReturnType<RPC.Registry, 'package.get'>

export type GetPackageReq = PackageParams & {
  id: string
  otherVersions: 'short'
}
export type GetPackageRes = T.GetPackageResponse & {
  /** StartOS marks revisions a newer one supersedes; the brochure omits them. */
  otherVersions: {
    [version: string]: T.PackageInfoShort & { superseded?: boolean }
  }
}

export type GetPackagesReq = PackageParams & {
  id: null
  targetVersion: null
  sourceVersion: null
  otherVersions: 'short'
}

export type GetPackagesRes = {
  [id: T.PackageId]: GetPackageRes
}

function isSinglePackage(
  result: PackageResult,
): result is T.GetPackageResponse | T.GetPackageResponseFull {
  return Array.isArray(result.categories)
}

function shortPackage(
  result: T.GetPackageResponse | T.GetPackageResponseFull,
): GetPackageRes {
  if (!result.otherVersions) throw new Error('Invalid package response')
  return { ...result, otherVersions: result.otherVersions }
}

export function packageResponse(result: PackageResult): GetPackageRes {
  if (!isSinglePackage(result)) throw new Error('Expected a single package')
  return shortPackage(result)
}

export function packagesResponse(result: PackageResult): GetPackagesRes {
  if (isSinglePackage(result)) throw new Error('Expected a package map')
  return Object.fromEntries(
    Object.entries(result).map(([id, info]) => [id, shortPackage(info)]),
  )
}

type RegistryObservation = Partial<Pick<T.RegistryCache, 'status' | 'error'>>

export type StoreIdentity = RegistryObservation & {
  url: string
  name: string
}

export type Marketplace = Record<string, StoreDataWithUrl | null>

export type StoreData<P extends MarketplaceListing = MarketplaceListing> =
  RegistryObservation & {
    info: T.RegistryInfo
    packages: P[]
    /** Installable updates of installed packages; absent outside StartOS. */
    updates?: P[]
  }

/** A release as a registry listing shows it; `getPackage$` resolves the full package. */
export type MarketplaceListing = Pick<
  T.PackageVersionInfo,
  | 'title'
  | 'icon'
  | 'releaseNotes'
  | 'preDownloadAlert'
  | 'defaultLocale'
  | 'osVersion'
  | 'sourceVersion'
  | 'satisfies'
  | 'gitHash'
  | 's9pks'
> &
  Pick<GetPackageRes, 'categories' | 'firstPublishedAt'> & {
    id: T.PackageId
    version: string
    flavor: string | null
    description: Pick<T.Description, 'short'> & Partial<T.Description>
  }

export type MarketplacePkgBase = T.PackageVersionInfo & {
  id: T.PackageId
  version: string
  flavor: string | null
  /** Signing keys declared by a local archive, not authenticated signatures. */
  signers?: string[]
  signatures?: T.SideloadParams['signatures']
}

export type MarketplacePkg = MarketplacePkgBase &
  GetPackageRes &
  T.PackageVersionInfo

export type StoreDataWithUrl<
  P extends MarketplaceListing = MarketplaceListing,
> = StoreData<P> & { url: string }
