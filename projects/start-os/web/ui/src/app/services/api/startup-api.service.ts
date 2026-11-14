import { inject, Service } from '@angular/core'
import type { RPC, RpcClient, RpcStreamClient } from '@start9labs/start-core'
import { createRpcClient, createRpcStreamClient } from '@start9labs/start-core'
import {
  AuthKeyService,
  isRpcError,
  RpcError,
  RpcSocketService,
  rpcRequestBody,
} from '@start9labs/shared'
import type { Observable } from 'rxjs'
import type { WebsocketConfig } from './api.types'

/** Initialization and diagnostic admission does not imply enrolled authorization. */
export abstract class StartupApiService {
  abstract readonly rpc: RpcClient<RPC.Init & RPC.Diagnostic>
  abstract readonly streams: RpcStreamClient<RPC.Init & RPC.Diagnostic>
  abstract openWebsocket$<T>(
    guid: string,
    config?: WebsocketConfig<T>,
  ): Observable<T>
}

@Service()
export class StartupLiveApiService extends StartupApiService {
  private readonly sockets = inject(RpcSocketService)
  private readonly authKeys = inject(AuthKeyService)

  readonly rpc: StartupApiService['rpc'] = createRpcClient<
    RpcClient<RPC.Init & RPC.Diagnostic>
  >({
    call: async (method, params, options) => {
      const request = rpcRequestBody({
        method,
        params: params as Record<string, unknown>,
      })
      const lease = await this.authKeys.admissionLease()
      const body = await this.sockets.message(request, options ?? {}, lease)
      if (isRpcError(body)) throw new RpcError(body.error)
      return body.result
    },
  })

  readonly streams: StartupApiService['streams'] = createRpcStreamClient<
    StartupApiService['streams']
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

  openWebsocket$<T>(
    guid: string,
    config: WebsocketConfig<T> = {},
  ): Observable<T> {
    return this.sockets.open<T>(guid, config)
  }
}
