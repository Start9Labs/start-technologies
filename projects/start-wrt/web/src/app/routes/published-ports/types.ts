import type { AutomaticPortUseKind } from 'src/app/services/api/api.service'

export type Protocol = 'tcp' | 'udp' | 'tcp+udp'

export function isGua(ip: string): boolean {
  const firstHextet = parseInt(ip.split(':')[0], 16)
  return firstHextet >= 0x2000 && firstHextet <= 0x3fff
}

export type PublishedPortStatus =
  | 'active'
  | 'partial'
  | 'paused'
  | 'error'
  | 'disabled'

export interface PublishedPort {
  id: string
  enabled: boolean
  label: string
  deviceMac: string
  ports: string
  protocol: Protocol
  ipv4: boolean
  ipv6: boolean
  ipv4PublicPort?: string
  source: 'any' | string
  /**
   * Preserves prior WAN collision confirmation. An edit that changes the
   * published range or the protocol drops it, re-validating what the user
   * never confirmed.
   */
  overrideWanPorts?: boolean
}

export interface PublishedPortDialogResult {
  port: PublishedPort
  reserveIpv4: boolean
}

export interface PublishedPortDisplay extends PublishedPort {
  status: PublishedPortStatus
  statusReason?: string
  deviceName?: string
  deviceIpv4?: string
  deviceIpv6?: string
  endpointIpv4?: string
  endpointIpv6?: string
}

export interface AutomaticPortUseDisplay {
  id: string
  kind: AutomaticPortUseKind
  deviceMac: string
  deviceName?: string
  ports: string
  publicPorts: string
  expiresSecs?: number
  hostname?: string
}
