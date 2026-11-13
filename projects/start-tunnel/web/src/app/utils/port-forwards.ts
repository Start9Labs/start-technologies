import type { T } from '@start9labs/start-core'

export type ForwardProtocol = NonNullable<
  T.Tunnel.AddPortForwardParams['protocol']
>
export type SniForward = Extract<T.Tunnel.PortForward, { kind: 'sni' }>

const SNI_BINDING_KEYS = {
  tcp: 'tcp',
  udp: 'udp',
  'tcp+udp': 'both',
} as const satisfies Record<ForwardProtocol, 'tcp' | 'udp' | 'both'>

export const FORWARD_PROTOCOLS = Object.keys(
  SNI_BINDING_KEYS,
) as readonly ForwardProtocol[]

const TCP_FORWARD_PROTOCOLS = FORWARD_PROTOCOLS.filter(
  protocol => protocol !== 'udp',
)

export function sniBindings(
  forward: SniForward,
  protocol: ForwardProtocol,
): T.Tunnel.SniBindings {
  return forward[SNI_BINDING_KEYS[protocol]]
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
    TCP_FORWARD_PROTOCOLS.find(candidate =>
      sniRoute(forward, candidate, hostname),
    ) ??
    'tcp'
  )
}

export function portEightyForwarded(
  forwards: T.Tunnel.PortForwards,
  ip: string,
): boolean {
  return Object.entries(forwards).some(([source, forward]) => {
    const idx = source.lastIndexOf(':')
    if (idx < 0 || source.slice(0, idx) !== ip) return false
    if (
      forward.kind === 'sni' &&
      !TCP_FORWARD_PROTOCOLS.some(protocol => {
        const bindings = sniBindings(forward, protocol)
        return Object.keys(bindings.routes).length || bindings.fallback
      })
    )
      return false
    const start = Number(source.slice(idx + 1))
    const span = forward.kind === 'dnat' ? forward.count : 1
    return start <= 80 && 80 <= start + span - 1
  })
}
