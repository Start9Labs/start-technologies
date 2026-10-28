import { RPCErrorDetails } from '../types/rpc.types'
import { getRpcErrorMessage } from '../util/rpc.util'

export class RpcError {
  constructor(private readonly error: RPCErrorDetails) {}

  readonly code = this.error.code
  readonly message = `RPC ERROR: ${getRpcErrorMessage(this.error)}`
}
