import type { RpcStreamOptions } from '@start9labs/start-core'
import type { Observable } from 'rxjs'

import { RpcError } from '../classes/rpc-error'
import type { RPCOptions } from '../types/rpc.types'
import type { AuthKeyService } from './auth-key.service'
import type { HttpService } from './http.service'
import { rpcRequestBody } from './rpc-request'
import { rpcResponseResult, waitForPatchSequence } from './rpc-response'
import type { RpcSocketService } from './rpc-socket.service'
import type { ServerIdentityService } from './server-identity.service'

/** Missing keys retain public HTTP; device replacement cancels keyed replies. */
export async function localRpcRequest<T>(
  http: HttpService,
  sockets: RpcSocketService,
  authKeys: AuthKeyService,
  identity: ServerIdentityService,
  cache: Observable<{ id: number }>,
  options: RPCOptions,
): Promise<T> {
  const request = rpcRequestBody(options)
  const lease = await authKeys.lease()
  const body = lease
    ? await sockets.message<T>(
        request,
        { ...options, openObserver: identity.openObserver },
        lease,
      )
    : (await http.rpcMessage<T>(request, options)).body
  lease?.signal.throwIfAborted()
  const result = rpcResponseResult(body)
  await waitForPatchSequence(cache, body.meta, lease?.signal)
  lease?.signal.throwIfAborted()
  return result
}

/** Requires a device key; server authorization remains live. */
export function authenticatedRpcTransport(
  sockets: RpcSocketService,
  authKeys: AuthKeyService,
  identity: ServerIdentityService,
) {
  return {
    async stream<R = unknown, I = unknown, F = unknown>(
      method: string,
      params: unknown,
      options: RpcStreamOptions = {},
    ) {
      const body = rpcRequestBody({
        method,
        params: params as Record<string, unknown>,
      })
      const lease = await authKeys.lease()
      options.signal?.throwIfAborted()
      if (!lease)
        throw new RpcError({ code: 34, message: 'Unauthenticated', data: null })
      const signal = options.signal
        ? AbortSignal.any([lease.signal, options.signal])
        : lease.signal
      const response = await sockets
        .stream<R, I, F>(body, lease, {
          ...options,
          signal,
          openObserver: identity.openObserver,
        })
        .catch(error => {
          signal.throwIfAborted()
          throw error
        })
      signal.throwIfAborted()
      return { ...response, signal }
    },
  }
}
