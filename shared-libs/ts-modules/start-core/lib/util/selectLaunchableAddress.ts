import { Host, HostnameInfo, ServiceInterface } from '../osBindings'
import { filledAddress } from './filledAddress'

export type LauncherAccess = {
  accessType:
    | 'tor'
    | 'mdns'
    | 'localhost'
    | 'ipv4'
    | 'ipv6'
    | 'domain'
    | 'wan-ipv4'
  hostname: string
}

function origin(url: string | null | undefined): string {
  if (!url) return ''

  try {
    const { protocol, host } = new URL(url)
    return host ? `${protocol}//${host.toLowerCase()}` : ''
  } catch {
    return ''
  }
}

function isResolvable(
  h: HostnameInfo,
  accessType: LauncherAccess['accessType'],
) {
  if (h.metadata.kind === 'plugin')
    return h.metadata.packageId === 'tor' && accessType === 'tor'

  return accessType !== 'tor' || h.public
}

export function selectLaunchableAddress(
  ui: ServiceInterface,
  host: Host,
  access: LauncherAccess,
): string {
  const addresses = filledAddress(host, ui.addressInfo)
  if (!addresses.hostnames.length) return ''

  const preferredOrigin = origin(ui.preferredLauncherAddress)
  if (preferredOrigin) {
    for (const h of addresses.nonLocal.hostnames) {
      const url = addresses.toUrl(h)
      if (isResolvable(h, access.accessType) && origin(url) === preferredOrigin)
        return url
    }
  }

  const publicDomains = addresses.filter({
    kind: 'domain',
    visibility: 'public',
  })
  const wanIp = addresses.filter({ kind: 'ipv4', visibility: 'public' })
  const bestPublic = [publicDomains, wanIp].flatMap(h =>
    h.format('urlstring'),
  )[0]
  const privateDomains = addresses.filter({
    kind: 'domain',
    visibility: 'private',
  })
  const mdns = addresses.filter({ kind: 'mdns' })
  const bestPrivate = [privateDomains, mdns].flatMap(h =>
    h.format('urlstring'),
  )[0]

  let matching: string | undefined
  let onLan = false
  switch (access.accessType) {
    case 'ipv4':
      matching = addresses.nonLocal
        .filter({
          kind: 'ipv4',
          predicate: h => h.hostname === access.hostname,
        })
        .format('urlstring')[0]
      onLan = true
      break
    case 'ipv6':
      matching = addresses.nonLocal
        .filter({
          kind: 'ipv6',
          predicate: h => h.hostname === access.hostname,
        })
        .format('urlstring')[0]
      break
    case 'localhost':
      matching = addresses.filter({ kind: 'localhost' }).format('urlstring')[0]
      onLan = true
      break
    case 'mdns':
      matching = mdns.format('urlstring')[0]
      onLan = true
      break
    case 'domain':
      matching = publicDomains.format('urlstring')[0]
      break
    case 'tor':
      matching = addresses.filter({ pluginId: 'tor' }).format('urlstring')[0]
      break
    case 'wan-ipv4':
      matching = wanIp.format('urlstring')[0]
      break
  }

  if (matching) return matching
  if (onLan && bestPrivate) return bestPrivate
  if (bestPublic) return bestPublic
  if (bestPrivate) return bestPrivate
  return ''
}
