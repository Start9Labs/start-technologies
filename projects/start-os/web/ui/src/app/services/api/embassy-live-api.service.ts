import type { ApiClient, Params, WebsocketConfig } from './api.types'
import { DOCUMENT, inject, Injectable } from '@angular/core'
import { blake3 } from '@noble/hashes/blake3'
import {
  AuthKeyService,
  HttpOptions,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
} from '@start9labs/shared'
import { createRpcClient } from '@start9labs/start-core'
import { Dump } from 'patch-db-client'
import { Observable } from 'rxjs'
import { webSocket, WebSocketSubject } from 'rxjs/webSocket'
import {
  PATCH_CACHE,
  waitForPatchSequence,
} from 'src/app/services/patch-db/patch-db-source'
import { AuthService } from '../auth.service'
import { DataModel } from '../patch-db/data-model'
import { ApiService } from './embassy-api.service'

@Injectable()
export class LiveApiService extends ApiService {
  private readonly document = inject(DOCUMENT)
  private readonly http = inject(HttpService)
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

    // @ts-ignore
    this.document.defaultView.rpcClient = this
  }

  async uploadFile(guid: string, body: Blob): Promise<void> {
    await this.httpRequest({
      method: 'POST',
      body,
      url: `/rest/rpc/${guid}`,
      // Service-worker fetch events expire during long uploads.
      headers: { 'ngsw-bypass': 'true' },
      timeout: 0,
    })
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

  // websocket

  openWebsocket$<T>(
    guid: string,
    config: WebsocketConfig<T> = {},
  ): WebSocketSubject<T> {
    const { location } = this.document.defaultView!
    const protocol = location.protocol === 'http:' ? 'ws' : 'wss'
    const host = location.host

    return webSocket({
      url: `${protocol}://${host}/ws/rpc/${guid}`,
      ...config,
    })
  }

  // state

  async echo(params: Params<'echo'>, url: string): Promise<string> {
    return this.rpcRequest<string>({ method: 'echo', params }, url)
  }

  // db

  // auth

  // diagnostic

  // init

  // server

  async toggleKiosk(enable: boolean): Promise<null> {
    return this.rpcRequest({
      method: enable ? 'kiosk.enable' : 'kiosk.disable',
      params: {},
    })
  }

  // marketplace URLs

  // notification

  // proxies

  // wifi

  // smtp

  // ssh

  // backup

  // async addBackupTarget(
  //   type: BackupTargetType,
  //   params: RR.AddCifsBackupTargetReq | RR.AddCloudBackupTargetReq,
  // ): Promise<RR.AddBackupTargetRes> {
  //   params.path = params.path.replace('/\\/g', '/')
  //   return this.rpcRequest({ method: `backup.target.${type}.add`, params })
  // }

  // async updateBackupTarget(
  //   type: BackupTargetType,
  //   params: RR.UpdateCifsBackupTargetReq | RR.UpdateCloudBackupTargetReq,
  // ): Promise<RR.UpdateBackupTargetRes> {
  //   return this.rpcRequest({ method: `backup.target.${type}.update`, params })
  // }

  // async removeBackupTarget(
  //   params: RR.RemoveBackupTargetReq,
  // ): Promise<RR.RemoveBackupTargetRes> {
  //   return this.rpcRequest({ method: 'backup.target.remove', params })
  // }

  // async getBackupJobs(
  //   params: RR.GetBackupJobsReq,
  // ): Promise<RR.GetBackupJobsRes> {
  //   return this.rpcRequest({ method: 'backup.job.list', params })
  // }

  // async createBackupJob(
  //   params: RR.CreateBackupJobReq,
  // ): Promise<RR.CreateBackupJobRes> {
  //   return this.rpcRequest({ method: 'backup.job.create', params })
  // }

  // async updateBackupJob(
  //   params: RR.UpdateBackupJobReq,
  // ): Promise<RR.UpdateBackupJobRes> {
  //   return this.rpcRequest({ method: 'backup.job.update', params })
  // }

  // async deleteBackupJob(
  //   params: RR.DeleteBackupJobReq,
  // ): Promise<RR.DeleteBackupJobRes> {
  //   return this.rpcRequest({ method: 'backup.job.delete', params })
  // }

  // async getBackupRuns(
  //   params: RR.GetBackupRunsReq,
  // ): Promise<RR.GetBackupRunsRes> {
  //   return this.rpcRequest({ method: 'backup.runs.list', params })
  // }

  // async deleteBackupRuns(
  //   params: RR.DeleteBackupRunsReq,
  // ): Promise<RR.DeleteBackupRunsRes> {
  //   return this.rpcRequest({ method: 'backup.runs.delete', params })
  // }

  // package

  // async setServiceOutboundProxy(
  //   params: RR.SetServiceOutboundTunnelReq,
  // ): Promise<RR.SetServiceOutboundTunnelRes> {
  //   return this.rpcRequest({ method: 'package.proxy.set-outbound', params })
  // }

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
        this.auth.setUnverified()
      }
      throw new RpcError(body.error)
    }

    const patchSequence = res.headers.get('x-patch-sequence')
    if (patchSequence)
      await waitForPatchSequence(this.cache$, Number(patchSequence))

    return body.result
  }

  private async httpRequest<T>(opts: HttpOptions): Promise<T> {
    return (await this.httpResponse<T>(opts)).body
  }

  private async httpResponse<T>(opts: HttpOptions) {
    // Static asset routes require a signature.
    if (
      opts.url.startsWith('/s9pk') ||
      opts.url.startsWith('/registry/icons/')
    ) {
      opts = {
        ...opts,
        headers: {
          ...opts.headers,
          ...(await this.authKeys.signHeader(new Uint8Array(0))),
        },
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
