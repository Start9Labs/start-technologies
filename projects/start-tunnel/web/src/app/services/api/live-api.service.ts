import { DOCUMENT, inject, Injectable } from '@angular/core'
import {
  AuthKeyService,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
} from '@start9labs/shared'
import { createRpcClient } from '@start9labs/start-core'
import { filter, firstValueFrom, Observable } from 'rxjs'
import { webSocket } from 'rxjs/webSocket'
import { AuthService } from '../auth.service'
import { PATCH_CACHE } from '../patch-db/patch-db-source'
import { ApiClient, ApiService } from './api.service'

@Injectable({
  providedIn: 'root',
})
export class LiveApiService extends ApiService {
  private readonly http = inject(HttpService)
  private readonly document = inject(DOCUMENT)
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
    const { location } = this.document.defaultView!
    const host = location.host

    return webSocket({
      url: `wss://${host}/ws/rpc/${guid}`,
    })
  }

  private async rpcRequest<T>(
    options: RPCOptions<Record<string, unknown>, string>,
    urlOverride?: string,
  ): Promise<T> {
    // Foreign origins must never receive a signature valid at home.
    const res = await this.http.rpcRequest<T>(
      urlOverride
        ? options
        : {
            ...options,
            headers: {
              ...options.headers,
              ...(await this.authKeys.signRpcHeaders(options)),
            },
          },
      urlOverride,
    )
    const body = res.body

    if (isRpcError(body)) {
      if (body.error.code === 34) {
        console.error('Unauthenticated, logging out')
        this.auth.deauthenticate()
      }
      throw new RpcError(body.error)
    }

    const patchSequence = res.headers.get('x-patch-sequence')
    if (patchSequence)
      await firstValueFrom(
        this.cache$.pipe(filter(({ id }) => id >= Number(patchSequence))),
      )

    return body.result
  }
}
