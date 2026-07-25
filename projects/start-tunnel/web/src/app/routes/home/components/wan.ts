import { utils } from '@start9labs/start-core'
import { TunnelData } from 'src/app/services/patch-db/data-model'

type Gateways = TunnelData['gateways']

// Ranges core's is_wan_candidate
// (shared-libs/crates/start-core/src/net/port_map/upnp.rs) rejects: 0/8 +
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

// Addresses no host ever receives traffic on, so never a WAN.
const UNUSABLE_RANGES = [
  '0.0.0.0/8',
  '127.0.0.0/8',
  '169.254.0.0/16',
  '255.255.255.255/32',
].map(r => utils.IpNet.parse(r))

function inNone(ranges: readonly utils.IpNet[], address: string): boolean {
  const ip = utils.IpAddress.parse(address)
  return ip.isIpv4() && !ranges.some(r => r.contains(ip))
}

const isWanCandidate = (address: string) => inNone(NON_WAN_RANGES, address)

// The IPv4 addresses a subnet or device can egress from. Public ones wherever
// the host holds any; otherwise every address it holds, because a provider that
// translates the public address at its edge (AWS, Google Cloud, Azure, Oracle)
// leaves the host with only a private one — the address its traffic actually
// arrives on, and so the only one a published port can use. Mirrors core's
// default_wan_of (shared-libs/crates/start-core/src/tunnel/forward/igd.rs).
export function wanOptions(gateways: Gateways): readonly string[] {
  const held = Object.values(gateways).flatMap(
    g => g.ipInfo?.subnets.map(s => utils.IpNet.parse(s).address) ?? [],
  )
  const publicIps = held.filter(isWanCandidate)
  return publicIps.length
    ? publicIps
    : held.filter(ip => inNone(UNUSABLE_RANGES, ip))
}

// The address core resolves when a subnet or device pins none.
export function defaultWanIp(gateways: Gateways): string | null {
  for (const { ipInfo } of Object.values(gateways)) {
    if (ipInfo?.wanIp && isWanCandidate(ipInfo.wanIp)) return ipInfo.wanIp
  }
  return wanOptions(gateways)[0] ?? null
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
