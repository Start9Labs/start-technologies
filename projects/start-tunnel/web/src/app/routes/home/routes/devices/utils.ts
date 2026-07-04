import { Signal } from '@angular/core'
import { AbstractControl } from '@angular/forms'
import { T, utils } from '@start9labs/start-core'
import { IpNet } from '@start9labs/start-core/util'

export interface MappedDevice {
  readonly subnet: {
    readonly name: string
    readonly range: string
  }
  readonly ip: string
  readonly name: string
  readonly kind: T.Tunnel.WgClientKind
  readonly allowDnsInjection: boolean
  readonly allowAutoPortForward: boolean
  readonly wanIp: string | null
  readonly ipv6: string | null
}

export interface MappedSubnet {
  readonly range: string
  readonly name: string
  readonly clients: T.Tunnel.WgSubnetClients
  readonly wanIp: string | null
  readonly ipv6: string | null
}

// A device's IPv6, mirroring the backend `host_v6`: the subnet prefix's network
// octets with the device's 4 IPv4 octets OR'd into the low 4. `null` when the
// subnet has no prefix (or the inputs don't parse).
export function deviceIpv6(prefix: string | null, ip: string): string | null {
  if (!prefix) return null
  try {
    const net = utils.IpNet.parse(prefix)
    if (!net.isIpv6()) return null
    const octets = net.zero().octets.slice()
    const v4octets = utils.IpAddress.parse(ip).octets
    if (v4octets.length !== 4) return null
    // Clamp the IPv4 to the prefix's host space, mirroring the backend host_v6:
    // a /64 keeps the whole 32-bit IPv4, a /124 keeps only its low 4 bits.
    const keep = Math.min(128 - net.prefix, 32)
    let v4 = 0
    for (const o of v4octets) v4 = v4 * 256 + o
    if (keep < 32) v4 %= 2 ** keep
    octets[12] = (octets[12] ?? 0) | ((v4 >>> 24) & 0xff)
    octets[13] = (octets[13] ?? 0) | ((v4 >>> 16) & 0xff)
    octets[14] = (octets[14] ?? 0) | ((v4 >>> 8) & 0xff)
    octets[15] = (octets[15] ?? 0) | (v4 & 0xff)
    return utils.IpAddress.fromOctets(octets).address
  } catch {
    return null
  }
}

export interface DeviceData {
  readonly subnets: Signal<readonly MappedSubnet[]>
  readonly device?: MappedDevice
  readonly kind?: T.Tunnel.WgClientKind
  readonly wanOptions: readonly string[]
  readonly defaultWan: string | null
}

export function subnetValidator({ value }: AbstractControl<MappedSubnet>) {
  return !value?.clients || getIp(value)
    ? null
    : { noHosts: 'No hosts available' }
}

export const ipInSubnetValidator = (subnet: string | null = null) => {
  const ipnet = subnet && utils.IpNet.parse(subnet)
  return ({ value }: AbstractControl<string>) => {
    let ip: utils.IpAddress
    try {
      ip = utils.IpAddress.parse(value)
    } catch (e) {
      return { invalidIp: 'Not a valid IP Address' }
    }
    if (!ipnet) return null
    const zero = ipnet.zero().cmp(ip)
    const broadcast = ipnet.broadcast().cmp(ip)
    return zero + broadcast === 0
      ? null
      : zero === 0
        ? { isZeroAddr: `Address cannot be the zero address` }
        : broadcast === 0
          ? { isBroadcastAddress: `Address cannot be the broadcast address` }
          : { notInSubnet: `Address is not part of ${subnet}` }
  }
}

export function getIp({ clients, range, ipv6 }: MappedSubnet) {
  const net = IpNet.parse(range)
  const last = net.broadcast()

  // IPv6 addresses already taken by the server (.1, the subnet key's host) and
  // the existing clients. A candidate whose IPv6 collides is skipped, so the
  // suggested IP never trips the backend's uniqueness check.
  const takenV6 = ipv6
    ? new Set(
        [net.address, ...Object.keys(clients)]
          .map(ip => deviceIpv6(ipv6, ip))
          .filter((a): a is string => a !== null),
      )
    : null

  for (let ip = net.add(1); ip.cmp(last) === -1; ip = ip.add(1)) {
    if (clients[ip.address]) continue
    if (takenV6) {
      const v6 = deviceIpv6(ipv6, ip.address)
      if (v6 && takenV6.has(v6)) continue
    }
    return ip.address
  }

  return ''
}
