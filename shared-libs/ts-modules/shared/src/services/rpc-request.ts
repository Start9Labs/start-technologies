import { HttpHeaders } from '@angular/common/http'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'

import type { RPCOptions } from '../types/rpc.types'

/** Maps a versioned RPC URL to its sibling WebSocket route, preserving authority and query. */
export function rpcWebSocketUrl(
  base: string,
  origin: string,
  guid?: string,
): string {
  const url = new URL(base, origin)
  url.protocol = url.protocol === 'http:' ? 'ws:' : 'wss:'
  const segments = url.pathname.split('/')
  if (segments.at(-1) === '') segments.pop()
  segments.pop()
  segments.pop()
  url.pathname =
    segments.join('/') +
    SESSION_PROFILE.rpcPath +
    (guid === undefined ? '' : `/${encodeURIComponent(guid)}`)
  return url.href
}

export function rpcRequestBody({
  method,
  params,
}: Pick<RPCOptions<unknown>, 'method' | 'params'>): string {
  return JSON.stringify({ method, params })
}

export function rpcRequestHeaders(
  headers?: RPCOptions['headers'],
): HttpHeaders {
  return new HttpHeaders(headers).set('Content-Type', 'application/json')
}
