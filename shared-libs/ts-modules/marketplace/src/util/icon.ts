export { FALLBACK_ICON } from '@start9labs/shared'

/** Resolves a stored icon's path against the registry's `/icons/` route. */
export function registryIconUrl(registry: string, path: string): string {
  return new URL(`icons/${path}`, registry).href
}
