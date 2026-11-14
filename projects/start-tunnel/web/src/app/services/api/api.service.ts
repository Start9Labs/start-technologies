import { Injectable } from '@angular/core'
import { RPC, RpcClient } from '@start9labs/start-core'
import { legacyDatabaseSubscription } from '@start9labs/shared'
import { Update, Revision } from 'patch-db-client'
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

  watchDb$(): Observable<Update<TunnelData>> {
    return legacyDatabaseSubscription<TunnelData>(
      () => this.rpc.db.subscribe({}),
      guid => this.openWebsocket$<Revision>(guid),
    )
  }

  abstract openWebsocket$<T>(guid: string): Observable<T>
}
