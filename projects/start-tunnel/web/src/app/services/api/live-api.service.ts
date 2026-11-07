import { inject, Service } from '@angular/core'
import {
  AuthKeyService,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
  RpcSocketService,
  rpcRequestBody,
  waitForPatchSequence,
} from '@start9labs/shared'
import { createRpcClient } from '@start9labs/start-core'
import type { Observable } from 'rxjs'
import { AuthService } from '../auth.service'
import { PATCH_CACHE } from '../patch-db/patch-db-source'
import { ApiClient, ApiService } from './api.service'

@Service()
export class LiveApiService extends ApiService {
  private readonly http = inject(HttpService)
  private readonly sockets = inject(RpcSocketService)
  private readonly auth = inject(AuthService)
  private readonly authKeys = inject(AuthKeyService)
  private readonly cache$ = inject(PATCH_CACHE)
  readonly rpc = createRpcClient<ApiClient>({
    call: (method, params, options) =>
      this.rpcRequest({
        method,
        params: params as Record<string, unknown>,
        ...options,
      }),
  })

  constructor() {
    super()
  }

  openWebsocket$<T>(guid: string): Observable<T> {
    return this.sockets.open<T>(guid)
  }

  private async rpcRequest<T>(
    options: RPCOptions<Record<string, unknown>, string>,
    urlOverride?: string,
  ): Promise<T> {
    const request = rpcRequestBody(options)
    const lease = urlOverride ? null : await this.authKeys.lease()
    const body = lease
      ? await this.sockets.message<T>(request, options, lease)
      : (await this.http.rpcMessage<T>(request, options, urlOverride)).body

    if (isRpcError(body)) {
      if (body.error.code === 34) {
        console.error('Unauthenticated, logging out')
        this.auth.deauthenticate()
      }
      throw new RpcError(body.error)
    }

    if (!urlOverride) await waitForPatchSequence(this.cache$, body.meta)

    return body.result
  }
}
