import { sameUrl } from '@start9labs/shared'
import { T } from '@start9labs/start-core'

import { RegistryIdentity } from './types'

type Live = Partial<Pick<T.RegistryInfo, 'name' | 'icon' | 'description'>>

/**
 * What the marketplace shows for a registry: its listing where Start9 has
 * one, otherwise what the registry reports about itself.
 */
export function resolveRegistry(
  url: string,
  live: Live | null,
  known: readonly T.KnownRegistry[],
): RegistryIdentity {
  const listed = known.find(k => sameUrl(k.url, url))

  if (listed) {
    return {
      url,
      name: listed.name,
      icon: listed.icon,
      description: listed.description ?? live?.description ?? null,
      warning: listed.warning,
      listed: true,
      impersonating: false,
    }
  }

  const impersonating = !!live?.name && reserved(live.name, known)

  return {
    url,
    name: live?.name && !impersonating ? live.name : host(url),
    icon: live?.icon?.startsWith('data:image/') ? live.icon : null,
    description: live?.description ?? null,
    warning: null,
    listed: false,
    impersonating,
  }
}

function reserved(name: string, known: readonly T.KnownRegistry[]): boolean {
  const claimed = normalize(name)

  return (
    claimed.includes('start9') ||
    known.some(k => claimed.includes(normalize(k.name)))
  )
}

function normalize(name: string): string {
  return name.trim().toLowerCase().normalize('NFKC').replace(/\s+/g, ' ')
}

function host(url: string): string {
  try {
    return new URL(url).host
  } catch {
    return url
  }
}
