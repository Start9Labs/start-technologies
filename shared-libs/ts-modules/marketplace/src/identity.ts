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

export function resolveIcon(
  url: string,
  liveIcon: string | null | undefined,
  known: readonly T.KnownRegistry[],
): string | null {
  const fallback = pinnedIcon(url, known)
  if (!liveIcon) return fallback

  const live = dataUrl(liveIcon)
  if (!live?.mime.startsWith('image/')) return fallback

  const published = findKnown(url, known)
  return !published || iconsMatch(published.icon, liveIcon)
    ? liveIcon
    : fallback
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

  return iconsMatch(info.icon, known.icon)
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

function iconsMatch(a: string | null, b: string | null): boolean {
  if (a === null || b === null) return a === null && b === null

  const x = dataUrl(a)
  const y = dataUrl(b)

  return (
    !!x &&
    !!y &&
    x.mime === y.mime &&
    x.bytes.length === y.bytes.length &&
    x.bytes.every((value, i) => value === y.bytes[i])
  )
}

function dataUrl(url: string): { mime: string; bytes: Uint8Array } | null {
  const comma = url.indexOf(',')

  if (!url.startsWith('data:') || comma < 0) return null

  const metadata = url.slice(5, comma).split(';')
  const mime = metadata.shift()?.trim().toLowerCase() || ''
  const body = url.slice(comma + 1)

  try {
    const bytes = metadata.some(value => value.toLowerCase() === 'base64')
      ? Uint8Array.from(atob(body), c => c.charCodeAt(0))
      : new TextEncoder().encode(decodeURIComponent(body))

    return { mime, bytes }
  } catch {
    return null
  }
}
