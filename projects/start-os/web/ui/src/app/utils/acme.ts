export function toAuthorityName(
  url: string | null,
  addSsl = true,
): string | 'Root CA' | '-' {
  if (url) {
    return knownAuthorities.find(ca => ca.url === url)?.name || url
  } else {
    return addSsl ? 'Root CA' : '-'
  }
}

export function toAuthorityUrl(name: string): string {
  return knownAuthorities.find(ca => ca.name === name)?.url || name
}

/** Configured authorities: known ones in `knownAuthorities` order, then the rest by URL. */
export function orderedAuthorities(configured: readonly string[]): string[] {
  const rank = (url: string) => {
    const known = knownAuthorities.findIndex(ca => ca.url === url)
    return known === -1 ? Infinity : known
  }
  return [...configured].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b))
}

export function defaultAuthority(configured: readonly string[]): string {
  return orderedAuthorities(configured)[0] ?? 'local'
}

export const knownAuthorities = [
  {
    name: `Let's Encrypt`,
    url: 'https://acme-v02.api.letsencrypt.org/directory',
  },
  {
    name: `Let's Encrypt (Staging)`,
    url: 'https://acme-staging-v02.api.letsencrypt.org/directory',
  },
] as const
