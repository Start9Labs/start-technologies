import { inject, Service } from '@angular/core'
import {
  AuthKeyService,
  databaseSubscription,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
  RPCResponse,
  RpcSocketService,
  ServerIdentityService,
  rpcRequestBody,
  waitForPatchSequence,
} from '@start9labs/shared'
import { createRpcClient } from '@start9labs/start-core'
import { catchError } from 'rxjs'
import type { Observable } from 'rxjs'
import type { Update } from 'patch-db-client'
import type { TunnelData } from '../patch-db/data-model'
import { AuthService } from '../auth.service'
import { PATCH_CACHE } from '../patch-db/patch-db-source'
import { ApiClient, ApiService } from './api.service'

@Service()
export class LiveApiService extends ApiService {
  private readonly http = inject(HttpService)
  private readonly sockets = inject(RpcSocketService)
  private readonly identity = inject(ServerIdentityService)
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
    return this.sockets.open<T>(guid, {
      openObserver: this.identity.openObserver,
    })
  }

  override watchDb$(): Observable<Update<TunnelData>> {
    return databaseSubscription<TunnelData>(
      this.sockets,
      this.authKeys,
      this.identity,
    ).pipe(catchError(error => this.raiseError(error)))
  }

  private async rpcRequest<T>(
    options: RPCOptions<Record<string, unknown>, string>,
    urlOverride?: string,
  ): Promise<T> {
    const request = rpcRequestBody(options)
    const lease = urlOverride ? null : await this.authKeys.lease()
    const body = lease
      ? await this.sockets.message<T>(
          request,
          { ...options, openObserver: this.identity.openObserver },
          lease,
        )
      : (await this.http.rpcMessage<T>(request, options, urlOverride)).body

    const result = this.responseResult(body)
    if (!urlOverride) await waitForPatchSequence(this.cache$, body.meta)
    return result
  }

  private responseResult<T>(body: RPCResponse<T>): T {
    if (isRpcError(body)) this.raiseError(new RpcError(body.error))
    return body.result
  }

  private raiseError(error: unknown): never {
    if (error instanceof RpcError && error.code === 34) {
      console.error('Unauthenticated, logging out')
      this.auth.deauthenticate()
    }
    throw error
  }
}
