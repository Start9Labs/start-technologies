import type { RPCErrorDetails, RPCResponse } from '../types/rpc.types'
import { getRpcErrorMessage } from '../util/rpc.util'

export class RpcError {
  constructor(
    private readonly error: RPCErrorDetails,
    readonly meta?: RPCResponse<unknown>['meta'],
  ) {}

  readonly code = this.error.code
  readonly message = `RPC ERROR: ${getRpcErrorMessage(this.error)}`
}
