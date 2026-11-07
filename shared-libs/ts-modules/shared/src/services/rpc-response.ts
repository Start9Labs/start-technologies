import { filter, firstValueFrom } from 'rxjs'
import type { Observable } from 'rxjs'

import type { RPCResponse } from '../types/rpc.types'

/** Resolves after the cache reaches the reply's committed sequence. */
export async function waitForPatchSequence(
  cache: Observable<{ id: number }>,
  metadata: RPCResponse<unknown>['meta'],
): Promise<void> {
  const sequence = metadata?.patchSequence
  if (sequence != null)
    await firstValueFrom(cache.pipe(filter(({ id }) => id >= sequence)))
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
