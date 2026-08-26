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

/** Whether the port is reachable from outside for every family the gateway offers. */
export function externalAllPass(
  port: T.CheckPortRes | null | undefined,
  portV6: T.CheckPortV6Res | null | undefined,
  gua: string | null,
): boolean {
  if (!port) return false
  return port.openExternally && (!gua || !!portV6?.openExternally)
}

/** Whether the port is reachable from outside and from this network. */
export function portAllPass(
  port: T.CheckPortRes | null | undefined,
  portV6: T.CheckPortV6Res | null | undefined,
  gua: string | null,
): boolean {
  return externalAllPass(port, portV6, gua) && !!port?.hairpinning
}
