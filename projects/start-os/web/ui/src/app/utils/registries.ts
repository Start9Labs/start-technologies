import { defaultRegistries, sameUrl } from '@start9labs/shared'

function defaultRank(url: string): number {
  const defaults = Object.values(defaultRegistries)
  const rank = defaults.findIndex(known => sameUrl(known, url))
  return rank < 0 ? defaults.length : rank
}

/** Defaults in `defaultRegistries` order, then custom registries by name, then URL. */
export function sortedRegistries(
  registries: Record<string, string | null>,
): [string, string | null][] {
  return Object.entries(registries).sort(
    ([aUrl, aName], [bUrl, bName]) =>
      defaultRank(aUrl) - defaultRank(bUrl) ||
      (aName || aUrl).localeCompare(bName || bUrl) ||
      aUrl.localeCompare(bUrl),
  )
}
