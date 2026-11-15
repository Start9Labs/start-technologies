import { filter, firstValueFrom, fromEvent, map, takeUntil } from 'rxjs'
import type { Observable } from 'rxjs'

import { isRpcError } from '../types/rpc.types'
import type { RPCResponse } from '../types/rpc.types'
import { RpcError } from '../classes/rpc-error'

export function rpcResponseResult<T>(body: RPCResponse<T>): T {
  if (isRpcError(body)) throw new RpcError(body.error, body.meta)
  return body.result
}

/** Resolves at the committed sequence; cancellation rejects with the signal's reason. */
export async function waitForPatchSequence(
  cache: Observable<{ id: number }>,
  metadata: RPCResponse<unknown>['meta'],
  signal?: AbortSignal,
): Promise<void> {
  signal?.throwIfAborted()
  const sequence = metadata?.patchSequence
  if (sequence != null) {
    const reached = cache.pipe(filter(({ id }) => id >= sequence))
    await firstValueFrom(
      signal
        ? reached.pipe(
            takeUntil(
              fromEvent(signal, 'abort').pipe(
                map(() => {
                  throw signal.reason
                }),
              ),
            ),
          )
        : reached,
    )
  }
  signal?.throwIfAborted()
}

export function withLegacyPatchSequence<T>(
  response: RPCResponse<T>,
  header: string | null,
): RPCResponse<T> {
  if (response.meta?.patchSequence != null || header === null) return response

  return {
    ...response,
    meta: { ...response.meta, patchSequence: Number(header) },
  }
}
