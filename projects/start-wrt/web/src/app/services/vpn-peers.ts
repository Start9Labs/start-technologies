/** `ErrorKind::VpnPeersWouldBreak` in backend/ctrl/src/error.rs. */
export const VPN_PEERS_WOULD_BREAK_CODE = 1019

/** Whether a failed RPC means the change would delete inbound VPN peers. */
export function isVpnPeersWouldBreak(e: unknown): boolean {
  return (
    typeof e === 'object' &&
    e !== null &&
    (e as { code?: unknown }).code === VPN_PEERS_WOULD_BREAK_CODE
  )
}
