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

/** The first configured authority in `knownAuthorities` order, else any configured one. */
export function defaultAuthority(configured: readonly string[]): string {
  return (
    knownAuthorities.find(ca => configured.includes(ca.url))?.url ||
    configured[0] ||
    'local'
  )
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
