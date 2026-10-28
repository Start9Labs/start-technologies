import { RPCErrorDetails } from '../types/rpc.types'

export function getRpcErrorDetails(data: unknown): string | undefined {
  if (typeof data === 'string') return data
  if (
    data &&
    typeof data === 'object' &&
    'details' in data &&
    typeof data.details === 'string'
  )
    return data.details
  return undefined
}

export function getRpcErrorMessage(error: RPCErrorDetails): string {
  const details = getRpcErrorDetails(error.data)
  return details ? `${error.message}\n\n${details}` : error.message
}
