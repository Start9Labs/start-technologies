/** Returns whether an RPC failure represents an unreachable transport. */
export function isNetworkError(error: unknown): error is { code: 0 } {
  return (
    error !== null &&
    typeof error === 'object' &&
    'code' in error &&
    error.code === 0
  )
}

export function getErrorMessage(e: unknown): string {
  const error = e !== null && typeof e === 'object' ? e : null
  let message = ''

  if (typeof e === 'string') {
    message = e
  } else if (isNetworkError(e)) {
    message =
      "Request Error. Your browser couldn't reach your server (network or browser blocked the connection). Common causes: a VPN or firewall blocking loopback/LAN; the server is still starting; or the page is loaded from a different origin than the API. Try refreshing this page; if you're using a VPN, allow LAN connections or disconnect briefly."
  } else if (
    error &&
    'message' in error &&
    typeof error.message === 'string' &&
    error.message
  ) {
    message = error.message
  } else {
    message = 'Unknown Error'
  }

  return message
}
