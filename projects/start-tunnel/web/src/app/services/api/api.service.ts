import { Injectable } from '@angular/core'
import { RPC } from '@start9labs/start-core'
import { Dump } from 'patch-db-client'
import { Observable } from 'rxjs'
import { TunnelData } from '../patch-db/data-model'

export type Params<M extends RPC.RpcMethod<RPC.Tunnel>> = RPC.RpcParamType<
  RPC.Tunnel,
  M
>
export type Result<M extends RPC.RpcMethod<RPC.Tunnel>> = RPC.RpcReturnType<
  RPC.Tunnel,
  M
>

@Injectable({ providedIn: 'root' })
export abstract class ApiService {
  abstract openWebsocket$<T>(guid: string): Observable<T>
  abstract subscribe(): Promise<SubscribeRes>
  abstract login(params: Params<'auth.login'>): Promise<Result<'auth.login'>>
  abstract logout(): Promise<Result<'auth.logout'>>
  abstract setPassword(
    params: Params<'auth.set-password'>,
  ): Promise<Result<'auth.set-password'>>
  abstract addSubnet(
    params: Params<'subnet.add'>,
  ): Promise<Result<'subnet.add'>>
  abstract editSubnet(
    params: Params<'subnet.add'>,
  ): Promise<Result<'subnet.add'>>
  abstract deleteSubnet(
    params: Params<'subnet.remove'>,
  ): Promise<Result<'subnet.remove'>>
  abstract setSubnetDns(
    params: Params<'subnet.set-dns'>,
  ): Promise<Result<'subnet.set-dns'>>
  abstract setSubnetWan(
    params: Params<'subnet.set-wan'>,
  ): Promise<Result<'subnet.set-wan'>>
  abstract addDevice(
    params: Params<'device.add'>,
  ): Promise<Result<'device.add'>>
  abstract editDevice(
    params: Params<'device.add'>,
  ): Promise<Result<'device.add'>>
  abstract deleteDevice(
    params: Params<'device.remove'>,
  ): Promise<Result<'device.remove'>>
  abstract showDeviceConfig(
    params: Params<'device.show-config'>,
  ): Promise<Result<'device.show-config'>>
  abstract setDnsInjection(
    params: Params<'device.set-dns-injection'>,
  ): Promise<Result<'device.set-dns-injection'>>
  abstract setAutoPortForward(
    params: Params<'device.set-auto-port-forward'>,
  ): Promise<Result<'device.set-auto-port-forward'>>
  abstract setDeviceWan(
    params: Params<'device.set-wan'>,
  ): Promise<Result<'device.set-wan'>>
  abstract setDeviceKind(
    params: Params<'device.set-kind'>,
  ): Promise<Result<'device.set-kind'>>
  abstract addDnsRecord(params: Params<'dns.add'>): Promise<Result<'dns.add'>>
  abstract removeDnsRecord(
    params: Params<'dns.remove'>,
  ): Promise<Result<'dns.remove'>>
  abstract addForward(
    params: Params<'port-forward.add'>,
  ): Promise<Result<'port-forward.add'>>
  abstract deleteForward(
    params: Params<'port-forward.remove'>,
  ): Promise<Result<'port-forward.remove'>>
  abstract updateForwardLabel(
    params: Params<'port-forward.update-label'>,
  ): Promise<Result<'port-forward.update-label'>>
  abstract setForwardEnabled(
    params: Params<'port-forward.set-enabled'>,
  ): Promise<Result<'port-forward.set-enabled'>>
  abstract addPinhole(
    params: Params<'pinhole.add'>,
  ): Promise<Result<'pinhole.add'>>
  abstract deletePinhole(
    params: Params<'pinhole.remove'>,
  ): Promise<Result<'pinhole.remove'>>
  abstract updatePinholeLabel(
    params: Params<'pinhole.update-label'>,
  ): Promise<Result<'pinhole.update-label'>>
  abstract setPinholeEnabled(
    params: Params<'pinhole.set-enabled'>,
  ): Promise<Result<'pinhole.set-enabled'>>
  abstract setHttpRedirectEnabled(
    params: Params<'http-redirect.set-enabled'>,
  ): Promise<Result<'http-redirect.set-enabled'>>
  abstract setSubnetIpv6(
    params: Params<'subnet.set-ipv6'>,
  ): Promise<Result<'subnet.set-ipv6'>>
  abstract restart(): Promise<Result<'restart'>>
  abstract checkUpdate(): Promise<Result<'update.check'>>
  abstract applyUpdate(): Promise<Result<'update.apply'>>
}

export type SubscribeRes = Omit<Result<'db.subscribe'>, 'dump'> & {
  dump: Dump<TunnelData>
}
