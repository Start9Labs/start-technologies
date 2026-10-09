import type { RPCErrorDetails } from '../types/rpc.types'

export class RpcError extends Error {
  readonly code: number

  constructor(error: RPCErrorDetails) {
    let message: string

    if (typeof error.data === 'string') {
      message = `${error.message}\n\n${error.data}`
    } else {
      message = error.data?.details
        ? `${error.message}\n\n${error.data.details}`
        : error.message
    }

    super(`RPC ERROR: ${message}`)
    this.name = 'RpcError'
    this.code = error.code
  }
}
