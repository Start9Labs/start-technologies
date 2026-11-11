import { Service, Injector, inject } from '@angular/core'
import { HttpService } from './http.service'
import { AuthService } from './auth.service'
import {
  AuthKeyService,
  isRpcError,
  RpcError,
  rpcRequestHeaders,
  rpcRequestBody,
  RpcSocketService,
  ServerIdentityService,
} from '@start9labs/shared'
import type {
  RPCOptions as SharedRPCOptions,
  RPCResponse,
} from '@start9labs/shared'

export { isRpcError, RpcError } from '@start9labs/shared'
export type {
  RPCRequest,
  RPCSuccessRes,
  RPCErrorRes,
  RPCErrorDetails,
  RPCResponse,
} from '@start9labs/shared'
import type { FirmwareMetadata } from './api/events'
import { StaleUiService } from './stale-ui.service'
import type {
  Api,
  RpcMethod,
  RpcParamType,
  RpcReturnType,
} from './api/bindings'

@Service()
export class RpcService {
  private readonly http = inject(HttpService)
  private readonly authKeys = inject(AuthKeyService)
  private readonly injector = inject(Injector)
  private readonly sockets = inject(RpcSocketService)
  private readonly identity = inject(ServerIdentityService)
  private readonly staleUi = inject(StaleUiService)

  async request<M extends RpcMethod<Api>>(
    options: RPCOptions<M>,
  ): Promise<RpcReturnType<Api, M>> {
    const { headers, timeout } = options
    const request = rpcRequestBody(options)
    const lease = await this.authKeys.lease()
    const body = lease
      ? await this.sockets.message<RpcReturnType<Api, M>>(
          request,
          { ...options, openObserver: this.identity.openObserver },
          lease,
        )
      : (
          await this.http.request<RPCResponse<RpcReturnType<Api, M>>>({
            headers: rpcRequestHeaders(headers),
            body: request,
            timeout,
          })
        ).body
    const metadata = body?.meta as Partial<FirmwareMetadata> | undefined
    this.staleUi.checkHash(metadata?.gitHash)
    if (body === null) throw new Error('Empty RPC response')

    if (isRpcError(body)) {
      if (body.error.code === 34) {
        console.error('Unauthenticated, logging out')
        this.injector.get(AuthService).setUnverified()
      }
      throw new RpcError(body.error)
    }

    return body.result
  }
}

export type RPCOptions<M extends RpcMethod<Api>> = SharedRPCOptions<
  RpcParamType<Api, M>,
  M
>
