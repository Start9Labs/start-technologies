import { Injectable, Injector, inject } from '@angular/core'
import { HttpService } from './http.service'
import { AuthService } from './auth.service'
import { isRpcError, RpcError } from '@start9labs/shared'
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
import type {
  Api,
  RpcMethod,
  RpcParamType,
  RpcReturnType,
} from './api/bindings'

@Injectable({
  providedIn: 'root',
})
export class RpcService {
  private readonly http = inject(HttpService)
  private readonly injector = inject(Injector)

  async request<M extends RpcMethod<Api>>(
    options: RPCOptions<M>,
  ): Promise<RpcReturnType<Api, M>> {
    const { method, headers, params, timeout } = options

    const res = await this.http.request<RPCResponse<RpcReturnType<Api, M>>>({
      headers,
      body: { method, params },
      timeout,
    })

    const body = res.body
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
