import * as jose from 'node-jose'
import { RPC, T } from '@start9labs/start-core'
import { Observable } from 'rxjs'

export type Api = RPC.Setup | RPC.StartOS
export type Params<M extends RPC.RpcMethod<Api>> = RPC.RpcParamType<Api, M>
export type Result<M extends RPC.RpcMethod<Api>> = RPC.RpcReturnType<Api, M>

export abstract class ApiService {
  pubkey?: jose.JWK.Key

  abstract echo(params: Params<'echo'>, url: string): Promise<Result<'echo'>>
  abstract getStatus(): Promise<Result<'setup.status'>>
  abstract getPubKey(): Promise<void>
  abstract setKeyboard(
    params: Params<'setup.set-keyboard'>,
  ): Promise<Result<'setup.set-keyboard'>>
  abstract setLanguage(
    params: Params<'setup.set-language'>,
  ): Promise<Result<'setup.set-language'>>
  abstract getDisks(): Promise<Result<'setup.disk.list'>>
  abstract installOs(
    params: Params<'setup.install-os'>,
  ): Promise<Result<'setup.install-os'>>
  abstract attach(
    params: Params<'setup.attach'>,
  ): Promise<Result<'setup.attach'>>
  abstract execute(
    params: Params<'setup.execute'>,
  ): Promise<Result<'setup.execute'>>
  abstract verifyCifs(
    params: Params<'setup.cifs.verify'>,
  ): Promise<Result<'setup.cifs.verify'>>
  abstract complete(): Promise<Result<'setup.complete'>>
  abstract exit(): Promise<void>
  abstract shutdown(): Promise<void>
  abstract initFollowLogs(): Promise<Result<'setup.logs.follow'>>
  abstract openWebsocket$<T>(guid: string): Observable<T>
  abstract restart(): Promise<void>

  async encrypt(toEncrypt: string): Promise<T.EncryptedWire> {
    if (!this.pubkey) throw new Error('No pubkey found!')
    const encrypted = await jose.JWE.createEncrypt(this.pubkey)
      .update(toEncrypt)
      .final()
    return { encrypted }
  }
}
