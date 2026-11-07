import { DOCUMENT, inject, Service } from '@angular/core'
import type { RPC, RpcClient } from '@start9labs/start-core'
import { createRpcClient } from '@start9labs/start-core'
import {
  HttpService,
  isRpcError,
  RELATIVE_URL,
  RpcError,
  rpcWebSocketUrl,
} from '@start9labs/shared'
import type { Observable } from 'rxjs'
import { webSocket } from 'rxjs/webSocket'
import type { WebsocketConfig } from './api.types'

/** Unscoped initialization and diagnostic RPCs and their legacy continuations. */
export abstract class StartupApiService {
  abstract readonly rpc: RpcClient<RPC.Init & RPC.Diagnostic>
  abstract openWebsocket$<T>(
    guid: string,
    config?: WebsocketConfig<T>,
  ): Observable<T>
}

@Service()
export class StartupLiveApiService extends StartupApiService {
  private readonly http = inject(HttpService)
  private readonly document = inject(DOCUMENT)
  private readonly relativeUrl = inject(RELATIVE_URL)

  readonly rpc = createRpcClient<RpcClient<RPC.Init & RPC.Diagnostic>>({
    call: async (method, params, options) => {
      const { body } = await this.http.rpcRequest({
        method,
        params: params as Record<string, unknown>,
        ...options,
      })
      if (isRpcError(body)) throw new RpcError(body.error)
      return body.result
    },
  })

  openWebsocket$<T>(
    guid: string,
    config: WebsocketConfig<T> = {},
  ): Observable<T> {
    return webSocket<T>({
      ...config,
      url: rpcWebSocketUrl(
        this.relativeUrl,
        this.document.location.origin,
        guid,
      ),
    })
  }
}
