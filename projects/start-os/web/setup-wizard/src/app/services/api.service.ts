import type { RPC, RpcClient, RpcStreamClient } from '@start9labs/start-core'
import { Observable } from 'rxjs'

export type Api = RPC.Setup | RPC.StartOS
export type ApiClient = RpcClient<RPC.Setup> & RpcClient<RPC.StartOS>
export type Params<M extends RPC.RpcMethod<Api>> = RPC.RpcParamType<Api, M>
export type Result<M extends RPC.RpcMethod<Api>> = RPC.RpcReturnType<Api, M>

export abstract class ApiService {
  /** The server's RPC methods, as generated from its bindings. */
  abstract readonly rpc: ApiClient
  abstract readonly streams: RpcStreamClient<RPC.Setup>

  /** Reaches a server other than the one serving the UI. */
  abstract echo(params: Params<'echo'>, url: string): Promise<Result<'echo'>>
  abstract openWebsocket$<T>(guid: string): Observable<T>
}
