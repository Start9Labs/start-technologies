import { DOCUMENT, inject, Injectable } from '@angular/core'
import {
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
} from '@start9labs/shared'
import { createRpcClient } from '@start9labs/start-core'
import { Observable } from 'rxjs'
import { webSocket } from 'rxjs/webSocket'
import { ApiClient, ApiService, Params } from './api.service'

@Injectable({
  providedIn: 'root',
})
export class LiveApiService extends ApiService {
  private readonly http = inject(HttpService)
  private readonly document = inject(DOCUMENT)
  readonly rpc = createRpcClient<ApiClient>({
    call: (method, params, options) =>
      this.rpcRequest({
        method,
        params: params as Record<string, unknown>,
        ...options,
      }),
  })

  openWebsocket$<T>(guid: string): Observable<T> {
    const { location } = this.document.defaultView!
    const protocol = location.protocol === 'http:' ? 'ws' : 'wss'
    const host = location.host

    return webSocket({
      url: `${protocol}://${host}/ws/rpc/${guid}`,
    })
  }

  async echo(params: Params<'echo'>, url: string): Promise<string> {
    return this.rpcRequest<string>({ method: 'echo', params }, url)
  }

  private async rpcRequest<T>(
    opts: RPCOptions<Record<string, unknown>, string>,
    url?: string,
  ): Promise<T> {
    const res = await this.http.rpcRequest<T>(opts, url)
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
