import { DOCUMENT, inject, Injectable } from '@angular/core'
import {
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
} from '@start9labs/shared'
import { RPC } from '@start9labs/start-core'
import * as jose from 'node-jose'
import { Observable } from 'rxjs'
import { webSocket } from 'rxjs/webSocket'
import { Api, ApiService, Params } from './api.service'

@Injectable({
  providedIn: 'root',
})
export class LiveApiService extends ApiService {
  private readonly http = inject(HttpService)
  private readonly document = inject(DOCUMENT)

  openWebsocket$<T>(guid: string): Observable<T> {
    const { location } = this.document.defaultView!
    const protocol = location.protocol === 'http:' ? 'ws' : 'wss'
    const host = location.host

    return webSocket({
      url: `${protocol}://${host}/ws/rpc/${guid}`,
    })
  }

  async echo(params: Params<'echo'>, url: string): Promise<string> {
    return this.rpcRequest({ method: 'echo', params }, url)
  }

  async getStatus() {
    return this.rpcRequest({
      method: 'setup.status',
      params: {},
    })
  }

  async getPubKey() {
    const response = await this.rpcRequest({
      method: 'setup.get-pubkey',
      params: {},
    })
    this.pubkey = await jose.JWK.asKey(response)
  }

  async setKeyboard(params: Params<'setup.set-keyboard'>): Promise<null> {
    return this.rpcRequest({
      method: 'setup.set-keyboard',
      params,
    })
  }

  async setLanguage(params: Params<'setup.set-language'>): Promise<null> {
    return this.rpcRequest({
      method: 'setup.set-language',
      params,
    })
  }

  async getDisks() {
    return this.rpcRequest({
      method: 'setup.disk.list',
      params: {},
    })
  }

  async installOs(params: Params<'setup.install-os'>) {
    return this.rpcRequest({
      method: 'setup.install-os',
      params,
      timeout: 5 * 60 * 1000,
    })
  }

  async verifyCifs(source: Params<'setup.cifs.verify'>) {
    source.path = normalizeCifsPath(source.path)
    return this.rpcRequest({
      method: 'setup.cifs.verify',
      params: source,
    })
  }

  async attach(params: Params<'setup.attach'>) {
    return this.rpcRequest({
      method: 'setup.attach',
      params,
    })
  }

  async execute(params: Params<'setup.execute'>) {
    if (params.recoverySource?.type === 'backup') {
      const target = params.recoverySource.target
      if (target.type === 'cifs') {
        target.path = normalizeCifsPath(target.path)
      }
    }

    return this.rpcRequest({
      method: 'setup.execute',
      params,
    })
  }

  async initFollowLogs() {
    return this.rpcRequest({
      method: 'setup.logs.follow',
      params: {},
    })
  }

  async complete() {
    return this.rpcRequest({
      method: 'setup.complete',
      params: {},
    })
  }

  async exit() {
    await this.rpcRequest({
      method: 'setup.exit',
      params: {},
    })
  }

  async shutdown() {
    await this.rpcRequest({
      method: 'setup.shutdown',
      params: {},
    })
  }

  async restart() {
    await this.rpcRequest({
      method: 'setup.restart',
      params: {},
    })
  }

  private async rpcRequest<M extends RPC.RpcMethod<Api>>(
    opts: RPCOptions<RPC.RpcParamType<Api, M>, M>,
    url?: string,
  ): Promise<RPC.RpcReturnType<Api, M>> {
    const res = await this.http.rpcRequest<RPC.RpcReturnType<Api, M>>(opts, url)
    const rpcRes = res.body

    if (isRpcError(rpcRes)) {
      throw new RpcError(rpcRes.error)
    }

    return rpcRes.result
  }
}

function normalizeCifsPath(path: string): string {
  return path.replaceAll('\\', '/')
}
