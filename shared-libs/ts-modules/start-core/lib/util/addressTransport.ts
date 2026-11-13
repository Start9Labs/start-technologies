import type { BindOptions, HostnameInfo, Transport } from '../osBindings'

/**
 * Forwarding transports for a StartOS-managed address, not URL-plugin capabilities.
 * TLS and HTTP proxy listeners expose TCP.
 */
export function addressTransport(
  options: Pick<BindOptions, 'transport' | 'proxy'>,
  address: Pick<HostnameInfo, 'ssl'>,
): Transport {
  return address.ssl || options.proxy ? 'tcp' : options.transport
}

export const transportLabels = {
  tcp: 'TCP',
  udp: 'UDP',
  both: 'TCP/UDP',
} as const satisfies Record<Transport, string>
