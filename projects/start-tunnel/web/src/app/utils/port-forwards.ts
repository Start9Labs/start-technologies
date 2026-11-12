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
