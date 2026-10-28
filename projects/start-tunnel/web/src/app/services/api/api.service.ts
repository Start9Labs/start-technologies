import { Injectable } from '@angular/core'
import { RPC, RpcClient } from '@start9labs/start-core'
import { Dump } from 'patch-db-client'
import { Observable } from 'rxjs'
import { TunnelData } from '../patch-db/data-model'

export type Api = RPC.Tunnel
export type ApiClient = RpcClient<RPC.Tunnel>
export type Params<M extends RPC.RpcMethod<RPC.Tunnel>> = RPC.RpcParamType<
  RPC.Tunnel,
  M
>
export type Result<M extends RPC.RpcMethod<RPC.Tunnel>> = RPC.RpcReturnType<
  RPC.Tunnel,
  M
>

@Injectable({ providedIn: 'root' })
export abstract class ApiService {
  /** The server's RPC methods, as generated from its bindings. */
  abstract readonly rpc: ApiClient

  abstract openWebsocket$<T>(guid: string): Observable<T>
}

export type SubscribeRes = Omit<Result<'db.subscribe'>, 'dump'> & {
  dump: Dump<TunnelData>
}
