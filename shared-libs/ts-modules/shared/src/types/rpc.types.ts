import type { RPC } from '@start9labs/start-core'

interface RPCBase {
  jsonrpc: '2.0'
  id: string
}

export interface RPCRequest<T> extends RPCBase {
  method: string
  params?: T
}

export interface RPCSuccessRes<T> extends RPCBase {
  result: T
}

export interface RPCErrorRes extends RPCBase {
  error: RPCErrorDetails
}

export type RPCErrorDetails = RPC.RpcReturnType<
  RPC.Diagnostic,
  'diagnostic.error'
>

export type RPCResponse<T> = RPCSuccessRes<T> | RPCErrorRes

export interface RPCOptions<
  Params = Record<string, unknown>,
  Method extends string = string,
> {
  method: Method
  headers?: Record<string, string | string[]>
  params: Params
  timeout?: number
}

export function isRpcError<Error, Result>(
  arg: { error: Error } | { result: Result },
): arg is { error: Error } {
  return 'error' in arg
}
