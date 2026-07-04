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
    const octets = utils.IpNet.parse(prefix).zero().octets.slice()
    const v4 = utils.IpAddress.parse(ip).octets
    for (let i = 0; i < 4; i++) {
      octets[12 + i] = (octets[12 + i] ?? 0) | (v4[i] ?? 0)
    }
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

export function getIp({ clients, range }: MappedSubnet) {
  const net = IpNet.parse(range)
  const last = net.broadcast()

  for (let ip = net.add(1); ip.cmp(last) === -1; ip = ip.add(1)) {
    if (!clients[ip.address]) {
      return ip.address
    }
  }

  return ''
}
