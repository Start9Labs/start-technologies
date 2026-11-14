import { inject, Injectable } from '@angular/core'
import {
  AuthKeyService,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
  RpcSocketService,
  rpcRequestBody,
} from '@start9labs/shared'
import { createRpcClient, createRpcStreamClient } from '@start9labs/start-core'
import { Observable } from 'rxjs'
import { ApiClient, ApiService, Params } from './api.service'

@Injectable({
  providedIn: 'root',
})
export class LiveApiService extends ApiService {
  private readonly http = inject(HttpService)
  private readonly sockets = inject(RpcSocketService)
  private readonly authKeys = inject(AuthKeyService)
  readonly rpc = createRpcClient<ApiClient>({
    call: (method, params, options) =>
      this.rpcRequest({
        method,
        params: params as Record<string, unknown>,
        ...options,
      }),
  })

  readonly streams: ApiService['streams'] = createRpcStreamClient<
    ApiService['streams']
  >({
    stream: async (method, params, options) => {
      const body = rpcRequestBody({
        method,
        params: params as Record<string, unknown>,
      })
      const lease = await this.authKeys.admissionLease()
      options?.signal?.throwIfAborted()
      return this.sockets.stream(body, lease, options)
    },
  })

  openWebsocket$<T>(guid: string): Observable<T> {
    return this.sockets.open<T>(guid)
  }

  async echo(params: Params<'echo'>, url: string): Promise<string> {
    return this.rpcRequest<string>({ method: 'echo', params }, url)
  }

  private async rpcRequest<T>(
    opts: RPCOptions<Record<string, unknown>, string>,
    url?: string,
  ): Promise<T> {
    const request = rpcRequestBody(opts)
    const rpcRes = url
      ? (await this.http.rpcMessage<T>(request, opts, url)).body
      : await this.sockets.message<T>(
          request,
          opts,
          await this.authKeys.admissionLease(),
        )

    if (isRpcError(rpcRes)) {
      throw new RpcError(rpcRes.error)
    }

    return rpcRes.result
  }
}
