import type { ApiClient, Params, WebsocketConfig } from './api.types'
import { DOCUMENT, inject, Service } from '@angular/core'
import { blake3 } from '@noble/hashes/blake3'
import {
  AuthKeyService,
  HttpOptions,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
  RpcSocketService,
  rpcRequestBody,
  waitForPatchSequence,
} from '@start9labs/shared'
import { createRpcClient } from '@start9labs/start-core'
import { Dump } from 'patch-db-client'
import type { Observable } from 'rxjs'
import type { WebSocketSubject } from 'rxjs/webSocket'
import { PATCH_CACHE } from 'src/app/services/patch-db/patch-db-source'
import { AuthService } from '../auth.service'
import { DataModel } from '../patch-db/data-model'
import { ApiService, FileUploadOptions } from './embassy-api.service'

@Service()
export class LiveApiService extends ApiService {
  private readonly document = inject(DOCUMENT)
  private readonly http = inject(HttpService)
  private readonly sockets = inject(RpcSocketService)
  private readonly auth = inject(AuthService)
  private readonly authKeys = inject(AuthKeyService)
  private readonly cache$ = inject<Observable<Dump<DataModel>>>(PATCH_CACHE)
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

    Object.assign(this.document.defaultView!, { rpcClient: this })
  }

  async uploadFile(
    guid: string,
    body: Blob,
    options: FileUploadOptions = {},
  ): Promise<void> {
    await this.httpRequest({
      method: 'POST',
      body,
      url: this.restContinuationUrl(guid),
      // Service-worker fetch events expire during long uploads.
      headers: {
        'ngsw-bypass': 'true',
        ...(options.filename === undefined
          ? {}
          : { 'X-File-Name': encodeURIComponent(options.filename) }),
      },
      uploadProgress: options.progress,
      signal: options.signal,
      timeout: 0,
    })
  }

  downloadFileUrl(guid: string): string {
    return `${this.restContinuationUrl(guid)}?ngsw-bypass=true`
  }

  private restContinuationUrl(guid: string): string {
    return `/rest/rpc/${encodeURIComponent(guid)}`
  }

  async getStatic(
    urls: string[],
    params: Record<string, string | number>,
  ): Promise<string> {
    for (const url of urls) {
      try {
        const res = await this.httpRequest<string>({
          method: 'GET',
          url,
          params,
          responseType: 'text',
        })
        return res
      } catch (e) {}
    }
    throw new Error('Could not fetch static file')
  }

  async getStaticObjectUrl(url: string): Promise<string> {
    const { body, headers } = await this.httpResponse<ArrayBuffer>({
      method: 'GET',
      url,
      responseType: 'arraybuffer',
    })

    return URL.createObjectURL(
      new Blob([body], { type: headers.get('content-type') || '' }),
    )
  }

  openWebsocket$<T>(
    guid: string,
    config: WebsocketConfig<T> = {},
  ): WebSocketSubject<T> {
    return this.sockets.open<T>(guid, config)
  }

  async echo(params: Params<'echo'>, url: string): Promise<string> {
    return this.rpcRequest<string>({ method: 'echo', params }, url)
  }

  async toggleKiosk(enable: boolean): Promise<null> {
    return this.rpcRequest({
      method: enable ? 'kiosk.enable' : 'kiosk.disable',
      params: {},
    })
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
        this.auth.setUnverified()
      }
      throw new RpcError(body.error)
    }

    if (!urlOverride) await waitForPatchSequence(this.cache$, body.meta)

    return body.result
  }

  private async httpRequest<T>(opts: HttpOptions): Promise<T> {
    return (await this.httpResponse<T>(opts)).body
  }

  private async httpResponse<T>(opts: HttpOptions) {
    if (
      opts.url.startsWith('/s9pk') ||
      opts.url.startsWith('/registry/icons/')
    ) {
      opts = {
        ...opts,
        headers: await this.authKeys.signHeader(
          new Uint8Array(0),
          opts.headers,
        ),
      }
    }
    const res = await this.http.httpRequest<T>(opts)
    if (res.headers.get('File-Digest')) {
      const digest = res.headers.get('File-Digest')!
      let data: Uint8Array
      if (opts.responseType === 'arraybuffer') {
        data = Buffer.from(res.body as ArrayBuffer)
      } else if (opts.responseType === 'text') {
        data = Buffer.from(res.body as string)
      } else if ((opts.responseType as string) === 'blob') {
        data = Buffer.from(await (res.body as Blob).arrayBuffer())
      } else {
        console.warn(
          `could not verify File-Digest for responseType ${
            opts.responseType || 'json'
          }`,
        )
        return res
      }
      const [alg, hash] = digest.split('=', 2)
      if (alg === 'blake3') {
        if (
          Buffer.from(blake3(data)).compare(
            Buffer.from(hash?.replace(/:/g, '') || '', 'base64'),
          ) !== 0
        ) {
          throw new Error('File digest mismatch.')
        }
      } else {
        console.warn(`Unknown File-Digest algorithm ${alg}`)
      }
    }
    return res
  }
}
