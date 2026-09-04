import { inject, Injectable } from '@angular/core'
import { blake3 } from '@noble/hashes/blake3'
import {
  GetPackageReq,
  GetPackageRes,
  GetPackagesReq,
  GetPackagesRes,
  MarketplacePkg,
} from '@start9labs/marketplace'
import {
  HttpOptions,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { ApiService } from './api.service'

@Injectable()
export class LiveApiService extends ApiService {
  private readonly http = inject(HttpService)

  constructor() {
    super()

    // @ts-ignore
    window.rpcClient = this
  }

  async getRegistryInfo(registryUrl: string): Promise<T.RegistryInfo> {
    return this.registryRequest(registryUrl, {
      method: 'info',
      params: {},
    })
  }

  async getRegistryPackage(
    registryUrl: string,
    id: string,
    targetVersion: string | null,
  ): Promise<GetPackageRes> {
    const params: GetPackageReq = {
      id,
      targetVersion,
      sourceVersion: null,
      otherVersions: 'short',
    }

    return this.registryRequest<GetPackageRes>(registryUrl, {
      method: 'package.get',
      params,
    })
  }

  async getRegistryPackages(registryUrl: string): Promise<GetPackagesRes> {
    const params: GetPackagesReq = {
      id: null,
      targetVersion: null,
      sourceVersion: null,
      otherVersions: 'short',
    }

    return this.registryRequest<GetPackagesRes>(registryUrl, {
      method: 'package.get',
      params,
    })
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

  private async registryRequest<T>(
    registryUrl: string,
    options: RPCOptions,
  ): Promise<T> {
    // Hit the registry's RPC directly; the registry.* namespace + registry param is the StartOS host-proxy convention, absent on this static site.
    return this.rpcRequest(options, `${new URL('rpc/v0', registryUrl)}`)
  }

  private async rpcRequest<T>(
    options: RPCOptions,
    urlOverride?: string,
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
    if (res.headers.get('Repr-Digest')) {
      // verify
      const digest = res.headers.get('Repr-Digest')!
      let data: Uint8Array
      if (opts.responseType === 'arrayBuffer') {
        data = Buffer.from(res.body as ArrayBuffer)
      } else if (opts.responseType === 'text') {
        data = Buffer.from(res.body as string)
      } else if ((opts.responseType as string) === 'blob') {
        data = Buffer.from(await (res.body as Blob).arrayBuffer())
      } else {
        console.warn(
          `could not verify Repr-Digest for responseType ${
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
