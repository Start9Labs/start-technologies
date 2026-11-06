import type { RPCResponse } from '../types/rpc.types'

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
