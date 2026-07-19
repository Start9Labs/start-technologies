import { T, utils } from '@start9labs/start-core'

/**
 * The gateway's IPv6 global-unicast address (the DualStack `AAAA` target), or
 * null when it has none. Mirrors the backend, which serves a public domain over
 * this address whenever it is present.
 */
export function getGua(ipInfo: T.IpInfo): string | null {
  for (const cidr of ipInfo.subnets) {
    try {
      const net = utils.IpNet.parse(cidr)
      if (net.isGua()) return net.address
    } catch {}
  }
  return null
}

/**
 * Whether the domain's DNS resolves correctly for every family the gateway
 * offers: the `A` record must match the WAN IPv4 (if any) and the `AAAA` must
 * match the GUA (if any).
 */
export function dnsAllPass(
  dns: T.QueryDnsRes | null | undefined,
  wanIp: string | null,
  gua: string | null,
): boolean {
  if (!dns) return false
  return (!wanIp || dns.ipv4 === wanIp) && (!gua || dns.ipv6 === gua)
}

/**
 * Whether the port is reachable for every family the gateway offers: IPv4 open
 * externally + hairpinning, and (if a GUA exists) the v6 port open externally.
 * v6 is NAT-free, so it has no hairpinning requirement.
 */
export function portAllPass(
  port: T.CheckPortRes | null | undefined,
  portV6: T.CheckPortV6Res | null | undefined,
  gua: string | null,
): boolean {
  if (!port) return false
  const v4Ok = port.openExternally && port.hairpinning
  const v6Ok = !gua || !!portV6?.openExternally
  return v4Ok && v6Ok
}
