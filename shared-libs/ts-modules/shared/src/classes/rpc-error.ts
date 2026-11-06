import type { RPCErrorDetails } from '../types/rpc.types'
import { getRpcErrorMessage } from '../util/rpc.util'

export class RpcError extends Error {
  readonly code: number

  constructor(error: RPCErrorDetails) {
    super(`RPC ERROR: ${getRpcErrorMessage(error)}`)
    this.name = 'RpcError'
    this.code = error.code
  }
}
