import { HttpHeaders } from '@angular/common/http'
import { NOISE_PROFILE } from '@start9labs/start-core/osBindings/noise-profile'

import type { RPCOptions } from '../types/rpc.types'

/** Keeps the selected authority and query while choosing the WebSocket route. */
export function rpcWebSocketUrl(
  base: string,
  origin: string,
  guid?: string,
): string {
  const url = new URL(base, origin)
  url.protocol = url.protocol === 'http:' ? 'ws:' : 'wss:'
  url.pathname =
    NOISE_PROFILE.rpcPath +
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
