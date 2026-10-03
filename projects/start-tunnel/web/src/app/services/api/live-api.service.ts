import { DOCUMENT, inject, Injectable } from '@angular/core'
import {
  AuthKeyService,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
} from '@start9labs/shared'
import { RPC, T } from '@start9labs/start-core'
import { filter, firstValueFrom, Observable } from 'rxjs'
import { webSocket } from 'rxjs/webSocket'
import { AuthService } from '../auth.service'
import { PATCH_CACHE } from '../patch-db/patch-db-source'
import { ApiService, Params, Result, SubscribeRes } from './api.service'

type Api = RPC.Tunnel

@Injectable({
  providedIn: 'root',
})
export class LiveApiService extends ApiService {
  private readonly http = inject(HttpService)
  private readonly document = inject(DOCUMENT)
  private readonly auth = inject(AuthService)
  private readonly authKeys = inject(AuthKeyService)
  private readonly cache$ = inject(PATCH_CACHE)

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

  async subscribe(): Promise<SubscribeRes> {
    const response = await this.rpcRequest({
      method: 'db.subscribe',
      params: {},
    })
    // The omitted pointer selects the tunnel database root.
    return response as SubscribeRes
  }

  async login(params: Params<'auth.login'>): Promise<null> {
    return this.rpcRequest({ method: 'auth.login', params })
  }

  async logout(): Promise<Result<'auth.logout'>> {
    return this.rpcRequest({ method: 'auth.logout', params: {} })
  }

  async setPassword(params: Params<'auth.set-password'>): Promise<null> {
    return this.rpcRequest({ method: 'auth.set-password', params })
  }

  async addSubnet(params: Params<'subnet.add'>): Promise<null> {
    return this.upsertSubnet(params)
  }

  async editSubnet(params: Params<'subnet.add'>): Promise<null> {
    return this.upsertSubnet(params)
  }

  async deleteSubnet(params: Params<'subnet.remove'>): Promise<null> {
    return this.rpcRequest({ method: 'subnet.remove', params })
  }

  async setSubnetDns(params: Params<'subnet.set-dns'>): Promise<null> {
    return this.rpcRequest({ method: 'subnet.set-dns', params })
  }

  async setSubnetWan(params: Params<'subnet.set-wan'>): Promise<null> {
    return this.rpcRequest({ method: 'subnet.set-wan', params })
  }

  async addDevice(params: Params<'device.add'>): Promise<null> {
    return this.upsertDevice(params)
  }

  async editDevice(params: Params<'device.add'>): Promise<null> {
    return this.upsertDevice(params)
  }

  async deleteDevice(params: Params<'device.remove'>): Promise<null> {
    return this.rpcRequest({ method: 'device.remove', params })
  }

  async showDeviceConfig(
    params: Params<'device.show-config'>,
  ): Promise<string> {
    return this.rpcRequest({ method: 'device.show-config', params })
  }

  async setDnsInjection(
    params: Params<'device.set-dns-injection'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'device.set-dns-injection', params })
  }

  async setAutoPortForward(
    params: Params<'device.set-auto-port-forward'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'device.set-auto-port-forward', params })
  }

  async setDeviceWan(params: Params<'device.set-wan'>): Promise<null> {
    return this.rpcRequest({ method: 'device.set-wan', params })
  }

  async setDeviceKind(params: Params<'device.set-kind'>): Promise<null> {
    return this.rpcRequest({ method: 'device.set-kind', params })
  }

  async addDnsRecord(params: Params<'dns.add'>): Promise<null> {
    return this.rpcRequest({ method: 'dns.add', params })
  }

  async removeDnsRecord(params: Params<'dns.remove'>): Promise<null> {
    return this.rpcRequest({ method: 'dns.remove', params })
  }

  async addForward(params: Params<'port-forward.add'>): Promise<null> {
    return this.rpcRequest({ method: 'port-forward.add', params })
  }

  async deleteForward(params: Params<'port-forward.remove'>): Promise<null> {
    return this.rpcRequest({ method: 'port-forward.remove', params })
  }

  async updateForwardLabel(
    params: Params<'port-forward.update-label'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'port-forward.update-label', params })
  }

  async setForwardEnabled(
    params: Params<'port-forward.set-enabled'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'port-forward.set-enabled', params })
  }

  async addPinhole(params: Params<'pinhole.add'>): Promise<null> {
    return this.rpcRequest({ method: 'pinhole.add', params })
  }

  async deletePinhole(params: Params<'pinhole.remove'>): Promise<null> {
    return this.rpcRequest({ method: 'pinhole.remove', params })
  }

  async updatePinholeLabel(
    params: Params<'pinhole.update-label'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'pinhole.update-label', params })
  }

  async setPinholeEnabled(
    params: Params<'pinhole.set-enabled'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'pinhole.set-enabled', params })
  }

  async setHttpRedirectEnabled(
    params: Params<'http-redirect.set-enabled'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'http-redirect.set-enabled', params })
  }

  async setSubnetIpv6(params: Params<'subnet.set-ipv6'>): Promise<null> {
    return this.rpcRequest({ method: 'subnet.set-ipv6', params })
  }

  async restart(): Promise<null> {
    return this.rpcRequest({ method: 'restart', params: {} })
  }

  async checkUpdate(): Promise<T.Tunnel.TunnelUpdateResult> {
    return this.rpcRequest({ method: 'update.check', params: {} })
  }

  async applyUpdate(): Promise<T.Tunnel.TunnelUpdateResult> {
    return this.rpcRequest({ method: 'update.apply', params: {} })
  }

  private async upsertSubnet(params: Params<'subnet.add'>): Promise<null> {
    return this.rpcRequest({ method: 'subnet.add', params })
  }

  private async upsertDevice(params: Params<'device.add'>): Promise<null> {
    return this.rpcRequest({ method: 'device.add', params })
  }

  private async rpcRequest<M extends RPC.RpcMethod<Api>>(
    options: RPCOptions<RPC.RpcParamType<Api, M>, M>,
    urlOverride?: string,
  ): Promise<RPC.RpcReturnType<Api, M>> {
    // Foreign origins must never receive a signature valid at home.
    const res = await this.http.rpcRequest<RPC.RpcReturnType<Api, M>>(
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
