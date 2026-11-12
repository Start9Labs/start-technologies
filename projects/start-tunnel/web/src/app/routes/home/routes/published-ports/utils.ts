import { Signal } from '@angular/core'
import { T } from '@start9labs/start-core'
import {
  FORWARD_PROTOCOLS,
  ForwardProtocol,
  sniBindings,
} from 'src/app/utils/port-forwards'

export interface MappedDevice {
  readonly ip: string
  readonly name: string
  // Absent when the subnet has no IPv6 prefix.
  readonly ipv6?: string | null
}

export type IpVersion = 'ipv4' | 'ipv6'
export const FORWARD_PROTOCOL_LABELS = {
  tcp: 'TCP',
  udp: 'UDP',
  'tcp+udp': 'TCP/UDP',
} as const satisfies Record<ForwardProtocol, string>

export interface MappedForward {
  readonly protocol: ForwardProtocol | null
  // For a v4 forward this is the WAN IP; for a v6 pinhole it's the client's GUA.
  readonly externalip: string
  // Stores start ports for range operations, not formatted spans.
  readonly externalport: string
  readonly device: MappedDevice
  readonly internalport: string
  readonly count: number
  readonly label: T.Tunnel.SniRoute['label']
  readonly enabled: T.Tunnel.SniRoute['enabled']
  readonly auto: T.Tunnel.SniRoute['auto']
  readonly sni: string | null
  readonly hostname: string | null
  readonly ipVersion: IpVersion
}

export interface PublishedPortsData {
  readonly ips: Signal<readonly string[]>
  readonly devices: Signal<readonly MappedDevice[]>
}

export function mapForwards(
  portForwards: T.Tunnel.PortForwards,
  devices: readonly MappedDevice[],
): MappedForward[] {
  return Object.entries(portForwards).flatMap(([source, forward]) =>
    forward.kind === 'sni'
      ? FORWARD_PROTOCOLS.flatMap(protocol => {
          const bindings = sniBindings(forward, protocol)
          return [
            ...Object.entries(bindings.routes).map(([hostname, route]) =>
              toRow(
                source,
                route.target,
                route.label,
                route.enabled,
                route.auto,
                hostname,
                1,
                protocol,
              ),
            ),
            ...(bindings.fallback
              ? [
                  toRow(
                    source,
                    bindings.fallback.target,
                    bindings.fallback.label,
                    bindings.fallback.enabled,
                    bindings.fallback.auto,
                    null,
                    1,
                    protocol,
                  ),
                ]
              : []),
          ]
        })
      : [
          toRow(
            source,
            forward.target,
            forward.label,
            forward.enabled,
            forward.auto,
            null,
            forward.count,
          ),
        ],
  )

  function toRow(
    source: string,
    target: string,
    label: string | null,
    enabled: boolean,
    auto: boolean,
    hostname: string | null,
    count: number,
    protocol: ForwardProtocol | null = null,
  ): MappedForward {
    const [externalip, externalport] = source.split(':')
    const [targetip, internalport] = target.split(':')

    return {
      protocol,
      externalip: externalip!,
      externalport: externalport!,
      device: devices.find(d => d.ip === targetip) ?? {
        ip: targetip!,
        name: targetip!,
        ipv6: null,
      },
      internalport: internalport!,
      count,
      label,
      enabled,
      auto,
      sni: hostname,
      hostname,
      ipVersion: 'ipv4',
    }
  }
}

export function mapPinholes(
  pinholes: T.Tunnel.Pinholes6,
  devices: readonly MappedDevice[],
): MappedForward[] {
  return Object.entries(pinholes).map(([key, ph]) => {
    const match = key.match(/^\[(.+)\]:(\d+)$/)
    const gua = match?.[1] ?? key
    const externalport = match?.[2] ?? ''
    const internalport =
      ph.internalPort != null ? String(ph.internalPort) : externalport

    return {
      protocol: null,
      externalip: gua,
      externalport,
      device: devices.find(d => d.ipv6 === gua) ?? {
        ip: gua,
        name: gua,
        ipv6: gua,
      },
      internalport,
      count: ph.count,
      label: ph.label,
      enabled: ph.enabled,
      auto: ph.auto,
      sni: null,
      hostname: null,
      ipVersion: 'ipv6',
    }
  })
}
