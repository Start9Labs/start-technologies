import { Service, Injector, inject } from '@angular/core'
import { AuthService } from './auth.service'
import {
  AuthKeyService,
  isRpcError,
  RpcError,
  rpcRequestBody,
  RpcSocketService,
  ServerIdentityService,
} from '@start9labs/shared'
import type { RPCOptions as SharedRPCOptions } from '@start9labs/shared'

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
  private readonly authKeys = inject(AuthKeyService)
  private readonly injector = inject(Injector)
  private readonly sockets = inject(RpcSocketService)
  private readonly identity = inject(ServerIdentityService)
  private readonly staleUi = inject(StaleUiService)

  async request<M extends RpcMethod<Api>>(
    options: RPCOptions<M>,
  ): Promise<RpcReturnType<Api, M>> {
    const request = rpcRequestBody(options)
    const lease = await this.authKeys.admissionLease()
    const body = await this.sockets.message<RpcReturnType<Api, M>>(
      request,
      {
        ...options,
        openObserver: 'key' in lease ? this.identity.openObserver : undefined,
      },
      lease,
    )
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
