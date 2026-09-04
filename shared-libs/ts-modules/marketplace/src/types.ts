import { T } from '@start9labs/start-core'

export type GetPackageReq = {
  id: string
  targetVersion: string | null
  sourceVersion: string | null
  otherVersions: 'short'
}
export type GetPackageRes = T.GetPackageResponse & {
  otherVersions: { [version: string]: T.PackageInfoShort }
}

export type GetPackagesReq = {
  id: null
  targetVersion: null
  sourceVersion: null
  otherVersions: 'short'
}

export type GetPackagesRes = {
  [id: T.PackageId]: GetPackageRes
}

export type StoreIdentity = {
  url: string
  name: string
  /** Start9 lists this registry, and its listing is what displays. */
  listed: boolean
}

export type RegistryIdentity = StoreIdentity & {
  icon: string | null
  description: T.LocaleString | null
  warning: T.LocaleString | null
  /** Unlisted, and presenting a name reserved for a listed registry. */
  impersonating: boolean
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
