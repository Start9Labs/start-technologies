import { GetPackageRes } from '@start9labs/marketplace'
import { T } from '@start9labs/start-core'

/**
 * Registries serve i18n metadata as `LocaleString` (`string | Record<lang,
 * string>`) and only collapse it to the active locale when the request carries
 * a StartOS `device_info` — which this static, hostless site never sends. So we
 * flatten every `LocaleString` leaf to a string here, at the single data-ingress
 * boundary, mirroring what the StartOS backend does for the embedded UI. Without
 * it the raw `Record` form reaches templates and renders as "[object Object]".
 *
 * Centralizing it here (rather than relying on per-template `| localize`) holds
 * the guarantee even if a component forgets to localize, and is the hook a
 * future in-app language selector re-runs to re-localize without re-fetching.
 */
export type Localize = (
  value: T.LocaleString,
  defaultLocale?: string | null,
) => string

export function localizeRegistryInfo(
  info: T.RegistryInfo,
  localize: Localize,
): T.RegistryInfo {
  return {
    ...info,
    categories: mapValues(info.categories, c => ({
      ...c,
      name: localize(c.name, info.defaultLocale),
    })),
  }
}

export function localizePackageRes(
  res: GetPackageRes,
  localize: Localize,
): GetPackageRes {
  return {
    ...res,
    best: mapValues(res.best, v => localizeVersionInfo(v, localize)),
    otherVersions:
      res.otherVersions &&
      mapValues(res.otherVersions, v => ({
        ...v,
        releaseNotes: localize(v.releaseNotes, v.defaultLocale),
      })),
  }
}

function localizeVersionInfo(
  v: T.PackageVersionInfo,
  localize: Localize,
): T.PackageVersionInfo {
  return {
    ...v,
    description: {
      short: localize(v.description.short, v.defaultLocale),
      long: localize(v.description.long, v.defaultLocale),
    },
    releaseNotes: localize(v.releaseNotes, v.defaultLocale),
    dependencyMetadata: mapValues(v.dependencyMetadata, d => ({
      ...d,
      title: d.title == null ? d.title : localize(d.title, v.defaultLocale),
      description:
        d.description == null
          ? d.description
          : localize(d.description, v.defaultLocale),
    })),
  }
}

const mapValues = <V, R>(
  record: Record<string, V>,
  fn: (value: V) => R,
): Record<string, R> =>
  Object.fromEntries(
    Object.entries(record).map(([key, value]) => [key, fn(value)]),
  )
