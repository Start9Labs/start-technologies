export type Protocol = 'tcp' | 'udp' | 'tcp+udp'

/**
 * Mirror of the backend `is_gua`: a Global Unicast Address (2000::/3) is the
 * only IPv6 scope reachable from the WAN. ULA (fc00::/7) and link-local
 * (fe80::/10) are not, so they can't back an IPv6 published port. Checks the
 * leading hextet against 2000::/3, matching the backend's parsed range check
 * (a malformed/empty leading hextet — e.g. from `::1` — yields NaN → false).
 */
export function isGua(ip: string): boolean {
  const firstHextet = parseInt(ip.split(':')[0], 16)
  return firstHextet >= 0x2000 && firstHextet <= 0x3fff
}

export type PublishedPortStatus =
  | 'active'
  | 'partial' // IPv4 unavailable (e.g., CGNAT)
  | 'paused' // Device offline or identity mismatch
  | 'error' // Failed to apply rule
  | 'disabled'

export interface PublishedPort {
  id: string // Unique identifier
  enabled: boolean
  label: string
  deviceMac: string // Link to device by MAC
  ports: string // Internal port/range (e.g., "8123" or "27015-27030")
  protocol: Protocol
  ipv4: boolean
  ipv6: boolean
  ipv4PublicPort?: string // External port for IPv4 (defaults to internal)
  source: 'any' | string // 'any' or CIDR like "203.0.113.0/24"
}

export interface PublishedPortDialogResult {
  port: PublishedPort
  reserveIpv4: boolean
  reserveIpv6: boolean
}

export interface PublishedPortDisplay extends PublishedPort {
  status: PublishedPortStatus
  statusReason?: string
  deviceName?: string
  deviceIpv4?: string
  deviceIpv6?: string
  endpointIpv4?: string // e.g., "example.ddns.net:8123"
  endpointIpv6?: string // e.g., "[2001:db8::50]:8123"
}
