import type { Exver } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { GetPackageRes, MarketplacePkgBase } from '../types'

export type Release = T.PackageInfoShort & { version: string }

type ReleasesOf = Pick<
  MarketplacePkgBase,
  'version' | 'flavor' | 'releaseNotes' | 'defaultLocale'
> &
  Partial<Pick<GetPackageRes, 'otherVersions'>>

/** The target, then same-flavor intervening releases, newest first. */
export function pendingReleases(
  exver: Exver,
  pkg: ReleasesOf,
  installed?: string | null,
): Release[] {
  const pending =
    installed && exver.getFlavor(installed) === pkg.flavor
      ? Object.entries(pkg.otherVersions || {})
          .flatMap(([version, { releaseNotes, ...info }]) =>
            releaseNotes ? [{ ...info, releaseNotes, version }] : [],
          )
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

/** Whether the installed version may migrate to this release. */
export function canMigrateFrom(
  exver: Exver,
  pkg: Pick<T.PackageVersionInfo, 'sourceVersion'>,
  installed: string,
): boolean {
  return exver.satisfies(installed, pkg.sourceVersion || '*')
}

/** Whether this release needs a StartOS outside the server's package compatibility range. */
export function requiresOsUpdate(
  exver: Exver,
  pkg: Pick<T.PackageVersionInfo, 'osVersion'>,
  packageVersionCompat: string,
): boolean {
  return !exver.versionSatisfies(pkg.osVersion, packageVersionCompat)
}
