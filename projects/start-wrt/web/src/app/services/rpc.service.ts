import { Service, Injector, inject } from '@angular/core'
import { AuthService } from './auth.service'
import { createRpcStreamClient } from '@start9labs/start-core'
import type { RpcStreamClient } from '@start9labs/start-core'
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

  readonly streams = createRpcStreamClient<RpcStreamClient<Api>>({
    stream: async (method, params, options) => {
      const body = rpcRequestBody({
        method,
        params: params as Record<string, unknown>,
      })
      try {
        const lease = await this.authKeys.admissionLease()
        options?.signal?.throwIfAborted()
        const result = await this.sockets.stream(body, lease, {
          ...options,
          openObserver: 'key' in lease ? this.identity.openObserver : undefined,
        })
        this.checkFirmware(result.meta)
        return result
      } catch (error) {
        return this.raiseError(error)
      }
    },
  })

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
    this.checkFirmware(body?.meta)
    if (body === null) throw new Error('Empty RPC response')

    if (isRpcError(body)) this.raiseError(new RpcError(body.error))

    return body.result
  }

  private checkFirmware(meta: unknown): void {
    this.staleUi.checkHash(
      (meta as Partial<FirmwareMetadata> | undefined)?.gitHash,
    )
  }

  private raiseError(error: unknown): never {
    if (error instanceof RpcError) {
      if (error.meta) this.checkFirmware(error.meta)
      if (error.code === 34) {
        console.error('Unauthenticated, logging out')
        this.injector.get(AuthService).setUnverified()
      }
    }
    throw error
  }
}

export type RPCOptions<M extends RpcMethod<Api>> = SharedRPCOptions<
  RpcParamType<Api, M>,
  M
>
