import { Injectable } from '@angular/core'
import type * as Bindings from './bindings'
import type * as Events from './events'
import type { Api, RpcParamType, RpcReturnType } from './bindings'

@Injectable({
  providedIn: 'root',
})
export abstract class ApiService {
  abstract login(params: LoginReq): Promise<RpcReturnType<Api, 'auth.login'>>
  abstract logout(): Promise<null>
  abstract exec(params: ExecReq): Promise<ExecRes>
  abstract getFile(params: GetFileReq): Promise<GetFileRes>
  abstract setFile(params: SetFileReq): Promise<null>
  abstract getUci(params: GetUciReq): Promise<GetUciRes>
  abstract setUci(params: SetUciReq): Promise<SetUciRes>
  abstract systemInfo(timeout?: number): Promise<SystemInfoRes>
  abstract systemNewerVersions(): Promise<VersionInfo[]>
  abstract systemUpdate(params: SystemUpdateReq): Promise<SystemUpdateRes>
  abstract systemRestart(): Promise<null>
  abstract setPassword(params: SetPasswordReq): Promise<null>
  abstract setPreferences(
    params: SetPreferencesReq,
  ): Promise<RpcReturnType<Api, 'system.set-preferences'>>
  abstract vpnServerList(): Promise<VpnServers>
  abstract vpnServerSet(params: VpnServerSetArgs): Promise<null>
  abstract vpnServerDelete(params: VpnServerDeleteArgs): Promise<null>
  abstract vpnServerPeerAdd(
    params: VpnServerPeerAddArgs,
  ): Promise<VpnServerPeerAddResponse>
  abstract vpnServerPeerDelete(params: VpnServerPeerDeleteArgs): Promise<null>
  abstract wifiGet(): Promise<WifiConfig>
  abstract wifiSet(params: WifiConfig): Promise<WifiSetResult>
  abstract wifiGeneratePassword(): Promise<string>
  abstract wifiRegulatory(): Promise<WifiRegulatory>
  abstract wifiBlackoutGet(): Promise<ScheduleWindow[]>
  abstract wifiBlackoutSet(params: ScheduleWindow[]): Promise<null>
  abstract profilesList(): Promise<ProfileId[]>
  abstract profileGet(params: ProfileIdOpt): Promise<SecurityProfile>
  abstract profileCreate(params: ProfileCreateInput): Promise<ProfileId>
  abstract profileUpdate(params: ProfileUpdateInput): Promise<ProfileId>
  abstract profileDelete(params: ProfileIdOpt): Promise<null>
  abstract profileScheduleGet(params: {
    interface: string
  }): Promise<ScheduleWindow[]>
  abstract profileScheduleSet(params: {
    interface: string
    windows: ScheduleWindow[]
  }): Promise<null>
  abstract setTimezone(params: { timezone: string }): Promise<null>
  abstract getTimezones(): Promise<string[]>
  abstract checkInitialized(): Promise<CheckInitializedRes>
  abstract setInitialPassword(
    params: SetInitialPasswordReq,
  ): Promise<RpcReturnType<Api, 'auth.set-initial-password'>>
  abstract setupStatus(): Promise<SetupStatusRes>
  abstract systemFactoryReset(): Promise<null>
  abstract systemLogs(): Promise<LogsResponse>
  abstract devicesList(): Promise<DeviceFromApi[]>
  abstract devicesUpdate(params: DeviceUpdateReq): Promise<null>
  abstract devicesSetAutoForward(params: {
    mac: string
    allow: boolean
  }): Promise<null>
  abstract devicesForget(params: { mac: string }): Promise<null>
  abstract devicesDataUsage(
    params: DeviceDataUsageReq,
  ): Promise<DataUsagePointFromApi[]>
  abstract lanIpv4Get(): Promise<LanIpv4Response>
  abstract lanIpv4Set(params: LanIpv4SetRequest): Promise<null>
  abstract lanIpv6Get(): Promise<LanIpv6Response>
  abstract lanIpv6Set(params: LanIpv6SetRequest): Promise<null>
  abstract wanIpv4Get(): Promise<WanIpv4Response>
  abstract wanIpv4Set(params: WanIpv4SetRequest): Promise<null>
  abstract wanIpv6Get(): Promise<WanIpv6Response>
  abstract wanIpv6Set(params: WanIpv6SetRequest): Promise<null>
  abstract wanMacGet(): Promise<WanMacResponse>
  abstract wanMacSet(params: WanMacSetRequest): Promise<null>
  abstract wanDnsGet(): Promise<WanDnsResponse>
  abstract wanDnsSet(params: WanDnsSetRequest): Promise<null>
  abstract wanDdnsGet(): Promise<WanDdnsResponse>
  abstract wanDdnsSet(params: WanDdnsSetRequest): Promise<null>
  abstract publishedPortsList(): Promise<PublishedPortFromApi[]>
  abstract publishedPortsSet(
    params: PublishedPortsSetRequest,
  ): Promise<PublishedPortsSetResult>
  abstract publishedPortsAutoList(): Promise<AutomaticPortUseFromApi[]>
  abstract vpnClientList(): Promise<OutboundVpn[]>
  abstract vpnClientCreate(
    params: OutboundVpnCreateRequest,
  ): Promise<OutboundVpnCreateResponse>
  abstract vpnClientUpdate(params: OutboundVpnUpdateRequest): Promise<null>
  abstract vpnClientDelete(params: OutboundVpnDeleteRequest): Promise<null>
  abstract vpnClientSetEnabled(
    params: OutboundVpnSetEnabledRequest,
  ): Promise<null>
  abstract sshKeysList(): Promise<SshKeyFromApi[]>
  abstract sshKeysAdd(params: SshKeysAddRequest): Promise<SshKeyFromApi>
  abstract ethernetGet(): Promise<EthernetConfig>
  abstract ethernetSet(params: EthernetSetConfig): Promise<EthernetSetResult>
  abstract sshKeysDelete(params: SshKeysDeleteRequest): Promise<null>
  abstract activityList(
    params?: ActivityListParams,
  ): Promise<ActivityListResponse>
  abstract activityDelete(params: { id: number }): Promise<null>
  abstract activityClear(): Promise<null>
  abstract backupCreate(): Promise<BackupCreateRes>
  abstract backupRestore(): Promise<BackupRestoreRes>
  abstract diagnosticsCreate(): Promise<DiagnosticsCreateRes>
}

// Continuation types
export type BackupCreateRes = RpcReturnType<Api, 'backup.create'>

export type BackupRestoreRes = RpcReturnType<Api, 'backup.restore'>

export type DiagnosticsCreateRes = RpcReturnType<Api, 'diagnostics.create'>

export type EthernetPort = RpcReturnType<Api, 'ethernet.get'>['ports'][string]

export type EthernetConfig = RpcReturnType<Api, 'ethernet.get'>

export type EthernetSetPort = RpcParamType<Api, 'ethernet.set'>['ports'][string]

export type EthernetSetConfig = RpcParamType<Api, 'ethernet.set'>

export type AffectedPublishedPort = Bindings.AffectedPublishedPort

export type EthernetSetResult = RpcReturnType<Api, 'ethernet.set'>

export type SshKeyFromApi = RpcReturnType<Api, 'ssh-keys.add'>

export type SshKeysAddRequest = RpcParamType<Api, 'ssh-keys.add'>

export type SshKeysDeleteRequest = RpcParamType<Api, 'ssh-keys.delete'>

export type LanIpv4Response = RpcReturnType<Api, 'lan.ipv4-get'>

export type LanIpv4SetRequest = RpcParamType<Api, 'lan.ipv4-set'>

export type LanIpv6Response = RpcReturnType<Api, 'lan.ipv6-get'>

export type LanIpv6SetRequest = RpcParamType<Api, 'lan.ipv6-set'>

export type LogEntry = Bindings.LogEntry

export type LogsResponse = RpcReturnType<Api, 'system.logs'>

export type LoginReq = RpcParamType<Api, 'auth.login'>

export type ExecReq = RpcParamType<Api, 'exec'>

export type ExecRes = RpcReturnType<Api, 'exec'>

export type GetFileReq = RpcParamType<Api, 'file.get'>

export type GetFileRes = RpcReturnType<Api, 'file.get'>

export type SetFileReq = RpcParamType<Api, 'file.set'>

export type GetUciReq = RpcParamType<Api, 'uci.get'>

export type GetUciRes = RpcReturnType<Api, 'uci.get'>

export type SetUciReq = RpcParamType<Api, 'uci.set'>

export type SetUciRes = RpcReturnType<Api, 'uci.set'>

export type SystemInfoRes = RpcReturnType<Api, 'system.info'>

export type VersionInfo = RpcReturnType<Api, 'system.newer-versions'>[number]

export type SystemUpdateReq = RpcParamType<Api, 'system.update'>

export type SystemUpdateRes = RpcReturnType<Api, 'system.update'>

export type FullProgress = Events.FullProgress

export type NamedProgress = Events.NamedProgress

export type Progress = Events.Progress

export type SetPasswordReq = RpcParamType<Api, 'auth.set-password'>

export type Theme = 'dark' | 'light' | 'system'

export type RemoteAccess = 'default' | 'never' | 'always'

export type SetPreferencesReq = Partial<{
  language: string
  theme: Theme
  remoteAccess: RemoteAccess
}>

export type VpnServerPeer = Bindings.VpnServerPeer

export type VpnServer = Bindings.VpnServer

export type VpnServerConfig = Bindings.VpnServerConfigInput

export interface VpnServerEndpoint {
  address: string
  label: string
}

export type VpnServers = RpcReturnType<Api, 'vpn-server.list'>

export type VpnServerSetArgs = RpcParamType<Api, 'vpn-server.set'>

export type VpnServerDeleteArgs = RpcParamType<Api, 'vpn-server.delete'>

export type VpnServerPeerAddArgs = RpcParamType<Api, 'vpn-server.peer-add'>

export type VpnServerPeerDeleteArgs = RpcParamType<
  Api,
  'vpn-server.peer-delete'
>

export type VpnServerPeerAddResponse = RpcReturnType<Api, 'vpn-server.peer-add'>

export type WifiRadio = Bindings.WifiRadio

export type WifiPassword = RpcReturnType<Api, 'wifi.get'>['passwords'][number]

export type WifiProfileId = Bindings.ProfileId

export type WifiConfig = RpcReturnType<Api, 'wifi.get'> &
  Pick<RpcParamType<Api, 'wifi.set'>, 'confirmPublishedPortDeletion'>

export type WifiSetResult = RpcReturnType<Api, 'wifi.set'>

export type WifiRegulatory = RpcReturnType<Api, 'wifi.regulatory'>

export type ScheduleWindow = RpcReturnType<Api, 'profiles.schedule-get'>[number]

export type ProfileId = RpcReturnType<Api, 'profiles.set'>

export type ProfileIdOpt = Bindings.ProfileIdOptInput

export type SecurityProfile = RpcReturnType<Api, 'profiles.get'>

export type LanAccess = RpcParamType<Api, 'profiles.create'>['lan_access']

export type WanAccess = Bindings.WanAccess

export type ProfileCreateInput = RpcParamType<Api, 'profiles.create'>

export type ProfileUpdateInput = RpcParamType<Api, 'profiles.set'> &
  Pick<ProfileId, 'interface' | 'vlan_tag'>

export type CheckInitializedRes = RpcReturnType<Api, 'auth.check-initialized'>

export type SetInitialPasswordReq = RpcParamType<
  Api,
  'auth.set-initial-password'
>

export type SetupStatusRes = RpcReturnType<Api, 'setup.status'>

export type SetupFlashReq = Events.FlashParamsInput

export type SetupFlashEvent = Events.SetupEvent

export type DeviceFromApi = RpcReturnType<Api, 'devices.list'>[number]

export type DeviceUpdateReq = RpcParamType<Api, 'devices.update'>

export type DeviceDataUsagePeriod = 'week' | 'month' | '3months'

export type DeviceDataUsageReq = RpcParamType<Api, 'devices.data-usage'>

export type DataUsagePointFromApi = RpcReturnType<
  Api,
  'devices.data-usage'
>[number]

export type WanIpv4Mode = 'dhcp' | 'static' | 'pppoe'

export type WanIpv4Response = RpcReturnType<Api, 'wan.ipv4-get'>

export type WanIpv4SetRequest = RpcParamType<Api, 'wan.ipv4-set'>

export type WanIpv6Mode = 'disabled' | 'slaac' | 'dhcpv6' | 'static' | '6rd'

export type WanIpv6Response = RpcReturnType<Api, 'wan.ipv6-get'>

export type WanIpv6SetRequest = RpcParamType<Api, 'wan.ipv6-set'>

export type WanMacStrategy = 'router' | 'custom'

export type WanMacResponse = RpcReturnType<Api, 'wan.mac-get'>

export type WanMacSetRequest = RpcParamType<Api, 'wan.mac-set'>

export type WanDnsMode = 'isp' | 'custom'

export type DnsServer = Bindings.DnsServer

export type WanDnsResponse = RpcReturnType<Api, 'wan.dns-get'>

export type WanDnsSetRequest = RpcParamType<Api, 'wan.dns-set'>

export type WanDdnsProvider =
  | 'dyndns'
  | 'noip'
  | 'cloudflare'
  | 'duckdns'
  | 'freedns'

export type WanDdnsResponse = RpcReturnType<Api, 'wan.ddns-get'>

export type WanDdnsSetRequest = RpcParamType<Api, 'wan.ddns-set'>

export type PublishedPortProtocol = 'tcp' | 'udp' | 'tcp+udp'

export type PublishedPortStatusValue =
  | 'active'
  | 'partial'
  | 'paused'
  | 'error'
  | 'disabled'

export type PublishedPortFromApi = RpcReturnType<
  Api,
  'published-ports.list'
>[number]

export type PublishedPortInputForApi = RpcParamType<
  Api,
  'published-ports.set'
>['ports'][number]

export type PublishedPortsSetRequest = RpcParamType<Api, 'published-ports.set'>

export type WanPortCollision = Bindings.WanPortCollision

export type SniPortUse = Bindings.SniPortUse

export type PublishedPortsSetResult = RpcReturnType<Api, 'published-ports.set'>

export type AutomaticPortUseKind = Bindings.AutomaticPortUse['kind']

export type AutomaticPortUseFromApi = RpcReturnType<
  Api,
  'published-ports.auto-list'
>[number]

export type OutboundVpn = RpcReturnType<Api, 'vpn-client.list'>[number]

export type OutboundVpnCreateRequest = RpcParamType<Api, 'vpn-client.create'>

export type OutboundVpnCreateResponse = RpcReturnType<Api, 'vpn-client.create'>

export type OutboundVpnUpdateRequest = RpcParamType<Api, 'vpn-client.update'>

export type OutboundVpnDeleteRequest = RpcParamType<Api, 'vpn-client.delete'>

export type OutboundVpnSetEnabledRequest = RpcParamType<
  Api,
  'vpn-client.set-enabled'
>

export type ActivityEntry = Bindings.ActivityEntry

export type ActivityListResponse = RpcReturnType<Api, 'activity.list'>

export type ActivityListParams = RpcParamType<Api, 'activity.list'>
