import type { BindOptions, HostnameInfo, Transport } from '../osBindings'

/** TLS and HTTP proxy listeners expose TCP independently of raw forwarding. */
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
