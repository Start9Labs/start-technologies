import { T } from '@start9labs/start-core'

export type ForwardProtocol = NonNullable<
  T.Tunnel.AddPortForwardParams['protocol']
>
export type SniForward = Extract<T.Tunnel.PortForward, { kind: 'sni' }>

const SNI_BINDING_KEYS = {
  tcp: null,
  udp: 'udp',
  'tcp+udp': 'both',
} as const satisfies Record<ForwardProtocol, 'udp' | 'both' | null>

export const FORWARD_PROTOCOLS = Object.keys(
  SNI_BINDING_KEYS,
) as readonly ForwardProtocol[]

export function sniBindings(
  forward: SniForward,
  protocol: ForwardProtocol,
): T.Tunnel.SniBindings {
  const key = SNI_BINDING_KEYS[protocol]
  return key ? forward[key] : forward
}

export function sniRoute(
  forward: SniForward,
  protocol: ForwardProtocol,
  hostname: string | null | undefined,
) {
  const bindings = sniBindings(forward, protocol)
  return hostname == null
    ? bindings.fallback
    : bindings.routes[hostname.toLowerCase()]
}

/** Omitted management selectors address the logical mapping that owns TCP. */
export function resolveSniProtocol(
  forward: SniForward,
  protocol: ForwardProtocol | null | undefined,
  hostname: string | null | undefined,
): ForwardProtocol {
  return (
    protocol ??
    FORWARD_PROTOCOLS.find(
      candidate =>
        candidate !== 'udp' && sniRoute(forward, candidate, hostname),
    ) ??
    'tcp'
  )
}
