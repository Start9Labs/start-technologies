import type { RPCErrorDetails, RPCResponse } from '../types/rpc.types'
import { getRpcErrorMessage } from '../util/rpc.util'

export class RpcError extends Error {
  readonly code: number

  constructor(
    error: RPCErrorDetails,
    readonly meta?: RPCResponse<unknown>['meta'],
  ) {
    super(`RPC ERROR: ${getRpcErrorMessage(error)}`)
    this.name = 'RpcError'
    this.code = error.code
  }
}
