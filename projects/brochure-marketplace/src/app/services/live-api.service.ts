import { inject, Injectable } from '@angular/core'
import { blake3 } from '@noble/hashes/blake3'
import { MarketplacePkg } from '@start9labs/marketplace'
import {
  HttpOptions,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
  registryUrl,
} from '@start9labs/shared'
import { createRpcClient } from '@start9labs/start-core'
import { ApiService, RegistryClient } from './api.service'

@Injectable()
export class LiveApiService extends ApiService {
  private readonly http = inject(HttpService)
  private readonly registries = new Map<string, RegistryClient>()

  constructor() {
    super()

    // @ts-ignore
    window.rpcClient = this
  }

  registry(url: string): RegistryClient {
    const root = `${new URL('rpc/v0', registryUrl(new URL(url).href))}`
    let client = this.registries.get(root)
    if (!client) {
      client = createRpcClient<RegistryClient>({
        call: (method, params, options) =>
          this.rpcRequest(
            { method, params: params as Record<string, unknown>, ...options },
            root,
          ),
      })
      this.registries.set(root, client)
    }
    return client
  }

  async getStaticProxy(
    pkg: MarketplacePkg,
    path: 'LICENSE.md' | 'instructions.md',
  ): Promise<string> {
    const encodedUrl = encodeURIComponent(pkg.s9pks[0]?.[1]?.urls[0] || '')

    return this.httpRequest({
      method: 'GET',
      url: `/s9pk/proxy/${encodedUrl}/${path}`,
      params: {
        rootSighash: pkg.s9pks[0]?.[1]?.commitment.rootSighash || '',
        rootMaxsize: pkg.s9pks[0]?.[1]?.commitment.rootMaxsize || '',
      },
      responseType: 'text',
    })
  }

  private async rpcRequest<T>(
    options: RPCOptions<Record<string, unknown>, string>,
    urlOverride: string,
  ): Promise<T> {
    const res = await this.http.rpcRequest<T>(options, urlOverride)
    const body = res.body

    if (isRpcError(body)) {
      throw new RpcError(body.error)
    }
    return body.result
  }

  private async httpRequest<T>(opts: HttpOptions): Promise<T> {
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
        return res.body
      }
      const computedDigest = Buffer.from(blake3(data)).toString('base64')
      if (`blake3=:${computedDigest}:` === digest) return res.body
      console.debug(computedDigest, digest)
      throw new Error('File digest mismatch.')
    }
    return res.body
  }
}
