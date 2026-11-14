import type { Exver } from '@start9labs/shared'
import type { MarketplacePkg } from '../types'

type Release = Pick<MarketplacePkg, 'version' | 'satisfies'> &
  Partial<Pick<MarketplacePkg, 'id' | 'satisfiesPackages'>>

export function satisfiesWithAliases(
  exver: Exver,
  pkg: Release,
  range: string,
  provides?: string | null,
) {
  if (!provides || provides === pkg.id) {
    return exver.releaseSatisfies(pkg.version, pkg.satisfies || [], range)
  }
  const [version, ...aliases] = pkg.satisfiesPackages?.[provides] || []
  return !!version && exver.releaseSatisfies(version, aliases, range)
}

export function compatibleVersions(
  exver: Exver,
  pkg: Release & Pick<MarketplacePkg, 'flavor' | 'otherVersions'>,
  range?: string | null,
  provides?: string | null,
) {
  return [pkg.version, ...Object.keys(pkg.otherVersions)]
    .filter(v => exver.getFlavor(v) === pkg.flavor)
    .filter(
      v =>
        !range ||
        (v === pkg.version
          ? satisfiesWithAliases(exver, pkg, range, provides)
          : (!provides || provides === pkg.id) && exver.satisfies(v, range)),
    )
    .sort((a, b) => -1 * (exver.compareExver(a, b) || 0))
}
