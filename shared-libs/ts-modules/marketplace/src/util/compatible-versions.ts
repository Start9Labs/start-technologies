import type { Exver } from '@start9labs/shared'
import type { MarketplacePkg } from '../types'

export function satisfiesWithAliases(
  exver: Exver,
  pkg: Pick<MarketplacePkg, 'version' | 'satisfies'>,
  range: string,
) {
  return exver.releaseSatisfies(pkg.version, pkg.satisfies || [], range)
}

export function compatibleVersions(
  exver: Exver,
  pkg: Pick<
    MarketplacePkg,
    'version' | 'satisfies' | 'flavor' | 'otherVersions'
  >,
  range?: string | null,
) {
  return [pkg.version, ...Object.keys(pkg.otherVersions)]
    .filter(v => exver.getFlavor(v) === pkg.flavor)
    .filter(
      v =>
        !range ||
        (v === pkg.version
          ? satisfiesWithAliases(exver, pkg, range)
          : exver.satisfies(v, range)),
    )
    .sort((a, b) => -1 * (exver.compareExver(a, b) || 0))
}
