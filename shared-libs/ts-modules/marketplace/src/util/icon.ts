export const FALLBACK_ICON = 'assets/img/service-icons/fallback.png'

export function registryIconUrl(
  registry: string,
  id: string,
  version: string,
  dependency?: string,
): string {
  const path = [
    id,
    version,
    ...(dependency ? ['dependencies', dependency] : []),
  ]
    .map(encodeURIComponent)
    .join('/')

  return new URL(`icons/${path}`, registry).href
}
