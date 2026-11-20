import type { Exver } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { GetPackageRes, MarketplacePkgBase } from '../types'

export type Release = T.PackageInfoShort & { version: string }

type ReleasesOf = MarketplacePkgBase &
  Partial<Pick<GetPackageRes, 'otherVersions'>>

/** This release, then the releases between it and the installed version, newest first. */
export function pendingReleases(
  exver: Exver,
  pkg: ReleasesOf,
  installed?: string | null,
): Release[] {
  const pending = installed
    ? Object.entries(pkg.otherVersions || {})
        .map(([version, info]) => ({ ...info, version }))
        .filter(
          ({ version }) =>
            exver.getFlavor(version) === pkg.flavor &&
            exver.compareExver(version, installed) === 1 &&
            exver.compareExver(version, pkg.version) === -1,
        )
        .sort((a, b) => exver.compareExver(b.version, a.version) || 0)
    : []
  return [pkg, ...pending]
}
