import { Injectable, DOCUMENT, inject } from '@angular/core'
import {
  HttpService,
  RPCOptions,
  isRpcError,
  RpcError,
  ErrorService,
} from '@start9labs/shared'
import { filter, firstValueFrom, Observable } from 'rxjs'
import { webSocket } from 'rxjs/webSocket'
import { T } from '@start9labs/start-core'
import { ApiService, SubscribeRes } from './api.service'
import { AuthService } from '../auth.service'
import { PATCH_CACHE } from '../patch-db/patch-db-source'

@Injectable({
  providedIn: 'root',
})
export class LiveApiService extends ApiService {
  private readonly http = inject(HttpService)
  private readonly document = inject(DOCUMENT)
  private readonly auth = inject(AuthService)
  private readonly cache$ = inject(PATCH_CACHE)
  private readonly errorService = inject(ErrorService)

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
    return this.rpcRequest({ method: 'db.subscribe', params: {} })
  }

  // auth

  async login(params: T.Tunnel.SetPasswordParams): Promise<null> {
    return this.rpcRequest({ method: 'auth.login', params })
  }

  async logout(): Promise<null> {
    return this.rpcRequest({ method: 'auth.logout', params: {} })
  }

  async setPassword(params: T.Tunnel.SetPasswordParams): Promise<null> {
    return this.rpcRequest({ method: 'auth.set-password', params })
  }

  async addSubnet(
    params: T.Tunnel.SubnetParams & T.Tunnel.AddSubnetParams,
  ): Promise<null> {
    return this.upsertSubnet(params)
  }

  async editSubnet(
    params: T.Tunnel.SubnetParams & T.Tunnel.AddSubnetParams,
  ): Promise<null> {
    return this.upsertSubnet(params)
  }

  async deleteSubnet(params: T.Tunnel.SubnetParams): Promise<null> {
    return this.rpcRequest({ method: 'subnet.remove', params })
  }

  async setSubnetDns(
    params: T.Tunnel.SubnetParams & T.Tunnel.SetSubnetDnsParams,
  ): Promise<null> {
    return this.rpcRequest({ method: 'subnet.set-dns', params })
  }

  async setSubnetWan(params: T.Tunnel.SetSubnetWanParams): Promise<null> {
    return this.rpcRequest({ method: 'subnet.set-wan', params })
  }

  // devices

  async addDevice(params: T.Tunnel.AddDeviceParams): Promise<null> {
    return this.upsertDevice(params)
  }

  async editDevice(params: T.Tunnel.AddDeviceParams): Promise<null> {
    return this.upsertDevice(params)
  }

  async deleteDevice(params: T.Tunnel.RemoveDeviceParams): Promise<null> {
    return this.rpcRequest({ method: 'device.remove', params })
  }

  async showDeviceConfig(params: T.Tunnel.RemoveDeviceParams): Promise<string> {
    return this.rpcRequest({ method: 'device.show-config', params })
  }

  async setDnsInjection(params: T.Tunnel.SetDnsInjectionParams): Promise<null> {
    return this.rpcRequest({ method: 'device.set-dns-injection', params })
  }

  async setAutoPortForward(
    params: T.Tunnel.SetAutoPortForwardParams,
  ): Promise<null> {
    return this.rpcRequest({ method: 'device.set-auto-port-forward', params })
  }

  async setDeviceWan(params: T.Tunnel.SetDeviceWanParams): Promise<null> {
    return this.rpcRequest({ method: 'device.set-wan', params })
  }

  async setDeviceKind(params: T.Tunnel.SetDeviceKindParams): Promise<null> {
    return this.rpcRequest({ method: 'device.set-kind', params })
  }

  // dns

  async addDnsRecord(params: T.Tunnel.AddDnsRecordParams): Promise<null> {
    return this.rpcRequest({ method: 'dns.add', params })
  }

  async removeDnsRecord(params: T.Tunnel.RemoveDnsRecordParams): Promise<null> {
    return this.rpcRequest({ method: 'dns.remove', params })
  }

  // forwards

  async addForward(params: T.Tunnel.AddPortForwardParams): Promise<null> {
    return this.rpcRequest({ method: 'port-forward.add', params })
  }

  async deleteForward(params: T.Tunnel.RemovePortForwardParams): Promise<null> {
    return this.rpcRequest({ method: 'port-forward.remove', params })
  }

  async updateForwardLabel(
    params: T.Tunnel.UpdatePortForwardLabelParams,
  ): Promise<null> {
    return this.rpcRequest({ method: 'port-forward.update-label', params })
  }

  async setForwardEnabled(
    params: T.Tunnel.SetPortForwardEnabledParams,
  ): Promise<null> {
    return this.rpcRequest({ method: 'port-forward.set-enabled', params })
  }

  // pinholes (IPv6)

  async addPinhole(params: T.Tunnel.AddPinholeParams): Promise<null> {
    return this.rpcRequest({ method: 'pinhole.add', params })
  }

  async deletePinhole(params: T.Tunnel.RemovePinholeParams): Promise<null> {
    return this.rpcRequest({ method: 'pinhole.remove', params })
  }

  async updatePinholeLabel(
    params: T.Tunnel.UpdatePinholeLabelParams,
  ): Promise<null> {
    return this.rpcRequest({ method: 'pinhole.update-label', params })
  }

  async setPinholeEnabled(
    params: T.Tunnel.SetPinholeEnabledParams,
  ): Promise<null> {
    return this.rpcRequest({ method: 'pinhole.set-enabled', params })
  }

  // ipv6

  async setSubnetIpv6(params: T.Tunnel.SetSubnetIpv6Params): Promise<null> {
    return this.rpcRequest({ method: 'subnet.set-ipv6', params })
  }

  // system

  async restart(): Promise<null> {
    return this.rpcRequest({ method: 'restart', params: {} })
  }

  // update

  async checkUpdate(): Promise<T.Tunnel.TunnelUpdateResult> {
    return this.rpcRequest({ method: 'update.check', params: {} })
  }

  async applyUpdate(): Promise<T.Tunnel.TunnelUpdateResult> {
    return this.rpcRequest({ method: 'update.apply', params: {} })
  }

  // private

  private async upsertSubnet(
    params: T.Tunnel.SubnetParams & T.Tunnel.AddSubnetParams,
  ): Promise<null> {
    return this.rpcRequest({ method: 'subnet.add', params })
  }

  private async upsertDevice(params: T.Tunnel.AddDeviceParams): Promise<null> {
    return this.rpcRequest({ method: 'device.add', params })
  }

  private async rpcRequest<T>(
    options: RPCOptions,
    urlOverride?: string,
  ): Promise<T> {
    const res = await this.http.rpcRequest<T>(options, urlOverride)
    const body = res.body

    if (isRpcError(body)) {
      if (body.error.code === 34) {
        console.error('Unauthenticated, logging out')
        this.auth.authenticated.set(false)
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
