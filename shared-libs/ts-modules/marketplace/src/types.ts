import { RPC, T } from '@start9labs/start-core'

type PackageParams = RPC.RpcParamType<RPC.Registry, 'package.get'>
type PackageResult = RPC.RpcReturnType<RPC.Registry, 'package.get'>

export type GetPackageReq = PackageParams & {
  id: string
  otherVersions: 'short'
}
export type GetPackageRes = T.GetPackageResponse & {
  otherVersions: { [version: string]: T.PackageInfoShort }
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

export type StoreIdentity = {
  url: string
  name: string
}

export type Marketplace = Record<string, StoreDataWithUrl | null>

export type StoreData = {
  info: T.RegistryInfo
  packages: MarketplacePkg[]
}

export type MarketplacePkgBase = T.PackageVersionInfo & {
  id: T.PackageId
  version: string
  flavor: string | null
}

export type MarketplacePkg = MarketplacePkgBase &
  GetPackageRes &
  T.PackageVersionInfo

export type StoreDataWithUrl = StoreData & { url: string }
