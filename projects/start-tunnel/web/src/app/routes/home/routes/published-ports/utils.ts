import { Signal } from '@angular/core'
import { T } from '@start9labs/start-core'

export interface MappedDevice {
  readonly ip: string
  readonly name: string
  // The device's IPv6 GUA (computed via the backend host_v6), or null/absent
  // when its subnet has no IPv6 prefix. Presence is what makes a v6 pinhole
  // possible. Optional so consumers that don't care about v6 (e.g. DNS) can omit it.
  readonly ipv6?: string | null
}

export type IpVersion = 'ipv4' | 'ipv6'

export interface MappedForward {
  // For a v4 forward this is the WAN IP; for a v6 pinhole it's the client's GUA.
  readonly externalip: string
  // Start port of the forward; `externalport`/`internalport` stay the raw start
  // (they rebuild the operation source key), while `count` gives the span.
  readonly externalport: string
  readonly device: MappedDevice
  readonly internalport: string
  readonly count: number
  readonly label: T.Tunnel.SniRoute['label']
  readonly enabled: T.Tunnel.SniRoute['enabled']
  readonly auto: T.Tunnel.SniRoute['auto']
  readonly sni: string | null
  readonly hostname: string | null
  // v4 (DNAT/SNI on the WAN IP) vs v6 (a firewall pinhole on the client's GUA).
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
      ? Object.entries(forward.routes).map(([hostname, route]) =>
          toRow(
            source,
            route.target,
            route.label,
            route.enabled,
            route.auto,
            hostname,
            1,
          ),
        )
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
  ): MappedForward {
    const [externalip, externalport] = source.split(':')
    const [targetip, internalport] = target.split(':')

    return {
      externalip: externalip!,
      externalport: externalport!,
      // Fall back to the raw target IP when it isn't a named device (e.g. a
      // manual SNI route to a non-client) so the row still renders rather than
      // crashing the whole table on an undefined device.
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

// v6 GUA pinholes keyed by `[gua]:port`. A pure pinhole has `internalPort` null
// (internal == external); a remap (e.g. 80→443) carries a distinct internal.
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
