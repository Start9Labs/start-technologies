import { Injectable } from '@angular/core'
import { T } from '@start9labs/start-core'
import { Dump } from 'patch-db-client'
import { Observable } from 'rxjs'
import { TunnelData } from '../patch-db/data-model'

@Injectable({
  providedIn: 'root',
})
export abstract class ApiService {
  abstract openWebsocket$<T>(guid: string): Observable<T>
  abstract subscribe(): Promise<SubscribeRes> // db.subscribe
  // auth
  abstract login(params: T.LoginParams): Promise<null> // auth.login
  abstract logout(): Promise<null> // auth.logout
  abstract setPassword(params: T.Tunnel.SetPasswordParams): Promise<null> // auth.set-password
  // subnets
  abstract addSubnet(
    params: T.Tunnel.SubnetParams & T.Tunnel.AddSubnetParams,
  ): Promise<null> // subnet.add
  abstract editSubnet(
    params: T.Tunnel.SubnetParams & T.Tunnel.AddSubnetParams,
  ): Promise<null> // subnet.edit
  abstract deleteSubnet(params: T.Tunnel.SubnetParams): Promise<null> // subnet.remove
  abstract setSubnetDns(
    params: T.Tunnel.SubnetParams & T.Tunnel.SetSubnetDnsParams,
  ): Promise<null> // subnet.set-dns
  abstract setSubnetWan(params: T.Tunnel.SetSubnetWanParams): Promise<null> // subnet.set-wan
  // devices
  abstract addDevice(params: T.Tunnel.AddDeviceParams): Promise<null> // device.add
  abstract editDevice(params: T.Tunnel.AddDeviceParams): Promise<null> // device.edit
  abstract deleteDevice(params: T.Tunnel.RemoveDeviceParams): Promise<null> // device.remove
  abstract showDeviceConfig(
    params: T.Tunnel.RemoveDeviceParams,
  ): Promise<string> // device.show-config
  abstract setDnsInjection(
    params: T.Tunnel.SetDnsInjectionParams,
  ): Promise<null> // device.set-dns-injection
  abstract setAutoPortForward(
    params: T.Tunnel.SetAutoPortForwardParams,
  ): Promise<null> // device.set-auto-port-forward
  abstract setDeviceWan(params: T.Tunnel.SetDeviceWanParams): Promise<null> // device.set-wan
  abstract setDeviceKind(params: T.Tunnel.SetDeviceKindParams): Promise<null> // device.set-kind
  // dns
  abstract addDnsRecord(params: T.Tunnel.AddDnsRecordParams): Promise<null> // dns.add
  abstract removeDnsRecord(
    params: T.Tunnel.RemoveDnsRecordParams,
  ): Promise<null> // dns.remove
  // forwards
  abstract addForward(params: T.Tunnel.AddPortForwardParams): Promise<null> // port-forward.add
  abstract deleteForward(
    params: T.Tunnel.RemovePortForwardParams,
  ): Promise<null> // port-forward.remove
  abstract updateForwardLabel(
    params: T.Tunnel.UpdatePortForwardLabelParams,
  ): Promise<null> // port-forward.update-label
  abstract setForwardEnabled(
    params: T.Tunnel.SetPortForwardEnabledParams,
  ): Promise<null> // port-forward.set-enabled

  abstract addPinhole(params: T.Tunnel.AddPinholeParams): Promise<null> // pinhole.add
  abstract deletePinhole(params: T.Tunnel.RemovePinholeParams): Promise<null> // pinhole.remove
  abstract updatePinholeLabel(
    params: T.Tunnel.UpdatePinholeLabelParams,
  ): Promise<null> // pinhole.update-label
  abstract setPinholeEnabled(
    params: T.Tunnel.SetPinholeEnabledParams,
  ): Promise<null> // pinhole.set-enabled

  // http redirects
  abstract setHttpRedirectEnabled(
    params: T.Tunnel.SetHttpRedirectEnabledParams,
  ): Promise<null> // http-redirect.set-enabled
  // ipv6
  abstract setSubnetIpv6(params: T.Tunnel.SetSubnetIpv6Params): Promise<null> // subnet.set-ipv6
  // system
  abstract restart(): Promise<null> // restart
  // update
  abstract checkUpdate(): Promise<T.Tunnel.TunnelUpdateResult> // update.check
  abstract applyUpdate(): Promise<T.Tunnel.TunnelUpdateResult> // update.apply
}

export type SubscribeRes = {
  dump: Dump<TunnelData>
  guid: string
}
