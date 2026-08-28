import { defaultIdentities, sameUrl } from '@start9labs/shared'
import { T } from '@start9labs/start-core'

import { StoreIdentity } from './types'

type Pin = { name: string; icon: string | null }

export function findKnown(
  url: string,
  known: readonly T.KnownRegistry[],
): T.KnownRegistry | undefined {
  return known.find(k => sameUrl(k.url, url))
}

export function pinnedIcon(
  url: string,
  known: readonly T.KnownRegistry[],
): string | null {
  return pin(url, known)?.icon || null
}

/**
 * Display identity for a registry: the pin where Start9 has one, otherwise the
 * name the registry reports — unless that name is claimed by a pin, in which
 * case the host stands in for it.
 */
export function resolveIdentity(
  url: string,
  liveName: string | null,
  known: readonly T.KnownRegistry[],
): StoreIdentity {
  const pinned = pin(url, known)

  if (pinned) {
    return { url, name: pinned.name, known: true }
  }

  const claimed = [
    ...known.map(k => k.name),
    ...Object.values(defaultIdentities).map(i => i.name),
  ]
  const impersonating = claimed.some(
    name => normalize(name) === normalize(liveName || ''),
  )

  return {
    url,
    name: liveName && !impersonating ? liveName : host(url),
    known: false,
  }
}

/** Whether a registry still presents the identity Start9 pinned for it. */
export function identityMatches(
  known: T.KnownRegistry,
  info: Pick<T.RegistryInfo, 'name' | 'icon'>,
): boolean {
  if (info.name !== known.name) return false

  return !info.icon || !known.icon || sameBytes(info.icon, known.icon)
}

function pin(url: string, known: readonly T.KnownRegistry[]): Pin | undefined {
  return (
    findKnown(url, known) ||
    Object.entries(defaultIdentities).find(([u]) => sameUrl(u, url))?.[1]
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

function sameBytes(a: string, b: string): boolean {
  const x = dataUrlBytes(a)
  const y = dataUrlBytes(b)

  return !!x && !!y && x.length === y.length && x.every((v, i) => v === y[i])
}

function dataUrlBytes(url: string): Uint8Array | null {
  const comma = url.indexOf(',')

  if (!url.startsWith('data:') || comma < 0) return null

  const body = url.slice(comma + 1)

  if (!url.slice(5, comma).split(';').includes('base64')) {
    return new TextEncoder().encode(decodeURIComponent(body))
  }

  try {
    return Uint8Array.from(atob(body), c => c.charCodeAt(0))
  } catch {
    return null
  }
}
