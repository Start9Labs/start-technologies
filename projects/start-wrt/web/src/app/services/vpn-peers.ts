/** `ErrorKind::VpnPeersWouldBreak` in backend/ctrl/src/error.rs. */
export const VPN_PEERS_WOULD_BREAK_CODE = 1019

const VPN_PEERS_WOULD_BREAK = 'VPN Peers Would Break'

/**
 * Whether a failed RPC means the change would delete inbound VPN peers. Matches
 * the kind code or the kind name leading the error detail.
 */
export function isVpnPeersWouldBreak(e: unknown): boolean {
  if (typeof e !== 'object' || e === null) return false
  const { code, message } = e as { code?: unknown; message?: unknown }
  if (code === VPN_PEERS_WOULD_BREAK_CODE) return true
  return (
    typeof message === 'string' &&
    message.includes(`\n\n${VPN_PEERS_WOULD_BREAK}:`)
  )
}
