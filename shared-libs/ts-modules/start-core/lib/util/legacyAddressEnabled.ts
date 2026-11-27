import { DerivedAddressInfo, HostnameInfo } from '../types'

function overridden(
  overrides: DerivedAddressInfo['disabled'],
  h: HostnameInfo,
): boolean {
  return overrides.some(
    ([hostname, port]) => hostname === h.hostname && port === (h.port ?? 0),
  )
}

function mdnsCovers(mdns: HostnameInfo, ip: HostnameInfo): boolean {
  return (
    mdns.metadata.kind === 'mdns' &&
    (ip.metadata.kind === 'ipv4' || ip.metadata.kind === 'ipv6') &&
    mdns.port === ip.port &&
    mdns.metadata.gateways.includes(ip.metadata.gateway)
  )
}

export type LegacyAddressSelections = Pick<
  DerivedAddressInfo,
  'enabled' | 'disabled' | 'lanEnabled' | 'available'
>

// Pre-projection servers supply overrides, without the default-off policy.
export function legacyAddressEnabled(
  addr: LegacyAddressSelections,
  h: HostnameInfo,
): boolean {
  if (h.public && (h.metadata.kind === 'ipv4' || h.metadata.kind === 'ipv6')) {
    if (h.port === null) return true
    const sa =
      h.metadata.kind === 'ipv6'
        ? `[${h.hostname}]:${h.port}`
        : `${h.hostname}:${h.port}`
    return addr.enabled.includes(sa)
  }
  if (h.metadata.kind === 'ipv4' || h.metadata.kind === 'ipv6') {
    if (overridden(addr.lanEnabled, h)) return true
    if (overridden(addr.disabled, h)) return false
    return addr.available
      .filter(mdns => mdnsCovers(mdns, h))
      .every(mdns => !overridden(addr.disabled, mdns))
  }
  if (!overridden(addr.disabled, h)) return true
  return (
    h.metadata.kind === 'mdns' &&
    !h.ssl &&
    addr.available.some(
      ip => !ip.public && mdnsCovers(h, ip) && legacyAddressEnabled(addr, ip),
    )
  )
}
