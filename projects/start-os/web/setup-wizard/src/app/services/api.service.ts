import * as jose from 'node-jose'
import { RPC, RpcClient, T } from '@start9labs/start-core'
import { Observable } from 'rxjs'

export type Api = RPC.Setup | RPC.StartOS
export type ApiClient = RpcClient<RPC.Setup> & RpcClient<RPC.StartOS>
export type Params<M extends RPC.RpcMethod<Api>> = RPC.RpcParamType<Api, M>
export type Result<M extends RPC.RpcMethod<Api>> = RPC.RpcReturnType<Api, M>

export abstract class ApiService {
  pubkey?: jose.JWK.Key

  /** The server's RPC methods, as generated from its bindings. */
  abstract readonly rpc: ApiClient

  /** Reaches a server other than the one serving the UI. */
  abstract echo(params: Params<'echo'>, url: string): Promise<Result<'echo'>>
  abstract openWebsocket$<T>(guid: string): Observable<T>

  async loadPubKey(): Promise<void> {
    this.pubkey = await jose.JWK.asKey(await this.rpc.setup.getPubkey({}))
  }

  async encrypt(toEncrypt: string): Promise<T.EncryptedWire> {
    if (!this.pubkey) throw new Error('No pubkey found!')
    const encrypted = await jose.JWE.createEncrypt(this.pubkey)
      .update(toEncrypt)
      .final()
    return { encrypted }
  }
}
