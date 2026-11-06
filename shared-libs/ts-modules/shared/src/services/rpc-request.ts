import { HttpHeaders } from '@angular/common/http'

import type { RPCOptions } from '../types/rpc.types'

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
