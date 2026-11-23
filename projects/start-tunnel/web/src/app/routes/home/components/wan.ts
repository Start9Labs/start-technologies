import { utils } from '@start9labs/start-core'
import { TunnelData } from 'src/app/services/patch-db/data-model'
import { entriesByKey } from 'src/app/utils/order'

type Gateways = TunnelData['gateways']

// Ranges core's is_wan_candidate (core/src/net/upnp.rs) rejects: 0/8 +
// unspecified, loopback, RFC1918, link-local, the TEST-NET docs ranges, and
// broadcast. CGNAT (100.64/10) is intentionally absent — the backend accepts it
// as a valid WAN, so the UI must too. Keep this list in sync with that fn.
const NON_WAN_RANGES = [
  '0.0.0.0/8',
  '127.0.0.0/8',
  '10.0.0.0/8',
  '172.16.0.0/12',
  '192.168.0.0/16',
  '169.254.0.0/16',
  '192.0.2.0/24',
  '198.51.100.0/24',
  '203.0.113.0/24',
  '255.255.255.255/32',
].map(r => utils.IpNet.parse(r))

function isWanCandidate(address: string): boolean {
  const ip = utils.IpAddress.parse(address)
  return ip.isIpv4() && !NON_WAN_RANGES.some(r => r.contains(ip))
}

// Public WAN IPv4 addresses the gateway can egress from — the explicit choices.
export function wanOptions(gateways: Gateways): readonly string[] {
  return entriesByKey(gateways)
    .map(([, gateway]) => gateway)
    .flatMap(
      g => g.ipInfo?.subnets.map(s => utils.IpNet.parse(s).address) ?? [],
    )
    .filter(isWanCandidate)
}

// Mirrors core's default_wan (core/src/tunnel/igd.rs): the first gateway's
// detected WAN IP if it is a candidate, else its first candidate subnet address.
export function defaultWanIp(gateways: Gateways): string | null {
  for (const [, { ipInfo }] of entriesByKey(gateways)) {
    if (!ipInfo) continue
    if (ipInfo.wanIp && isWanCandidate(ipInfo.wanIp)) return ipInfo.wanIp
    const subnet = ipInfo.subnets
      .map(s => utils.IpNet.parse(s).address)
      .find(isWanCandidate)
    if (subnet) return subnet
  }
  return null
}

// tuiSelect skips a bare `null` item, so the "default" choice is wrapped in an
// object to keep it selectable.
export interface WanItem {
  readonly ip: string | null
}

export function toWanItems(options: readonly string[]): readonly WanItem[] {
  return [{ ip: null }, ...options.map(ip => ({ ip }))]
}

export const matchWan = (a: WanItem, b: WanItem) => a.ip === b.ip

// `defaultLabel` names what the null/default option inherits from — "System
// default" (subnet) or "Subnet default" (device). When that default resolves to
// a known address, show it parenthetically, e.g. "System default (1.2.3.4)".
export function wanLabel(
  ip: string | null,
  defaultLabel: string,
  inheritedIp: string | null = null,
): string {
  if (ip) return ip
  return inheritedIp ? `${defaultLabel} (${inheritedIp})` : defaultLabel
}
