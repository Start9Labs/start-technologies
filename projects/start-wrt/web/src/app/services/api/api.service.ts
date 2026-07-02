import { Injectable } from '@angular/core'
import { UciFile } from './types'

@Injectable({
  providedIn: 'root',
})
export abstract class ApiService {
  abstract login(params: LoginReq): Promise<null>
  abstract logout(): Promise<null>
  abstract exec(params: ExecReq): Promise<ExecRes>
  abstract getFile(params: GetFileReq): Promise<GetFileRes>
  abstract setFile(params: SetFileReq): Promise<null>
  abstract getUci<T extends Record<string, UciFile<any>>>(
    params: GetUciReq,
  ): Promise<T>
  abstract setUci<T extends string[]>(params: SetUciReq): Promise<SetUciRes<T>>
  abstract systemInfo(timeout?: number): Promise<SystemInfoRes>
  abstract systemNewerVersions(): Promise<VersionInfo[]>
  abstract systemUpdate(params: SystemUpdateReq): Promise<SystemUpdateRes>
  abstract systemRestart(): Promise<null>
  abstract setPassword(params: SetPasswordReq): Promise<null>
  abstract setPreferences(params: SetPreferencesReq): Promise<null>
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
  abstract setInitialPassword(params: SetInitialPasswordReq): Promise<null>
  abstract setupStatus(): Promise<SetupStatusRes>
  abstract systemFactoryReset(): Promise<null>
  abstract systemLogs(): Promise<LogsResponse>
  abstract devicesList(): Promise<DeviceFromApi[]>
  abstract devicesUpdate(params: DeviceUpdateReq): Promise<null>
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
  abstract publishedPortsSet(params: PublishedPortsSetRequest): Promise<null>
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
export type BackupCreateRes = { guid: string; filename: string }
export type BackupRestoreRes = { upload: string }
export type DiagnosticsCreateRes = { guid: string; filename: string }

// Ethernet types
export interface EthernetPort {
  profile: ProfileId | null
}

export interface EthernetConfig {
  wan_ipv6: boolean
  wan_port: string | null
  ports: Record<string, EthernetPort>
}

export interface EthernetSetPort {
  profile: ProfileIdOpt | null
}

export interface EthernetSetConfig {
  wan_ipv6: boolean
  wan_port: string | null
  ports: Record<string, EthernetSetPort>
  // When true, authorize deleting the published ports listed in a prior
  // EthernetSetResult.pending_published_port_deletions response.
  confirm_published_port_deletion?: boolean
}

// A published port that will be deleted because its device is moving to a
// different security profile (snake_case fields — the backend serializes the
// shared AffectedPublishedPort verbatim in both ethernet and wifi results).
export interface AffectedPublishedPort {
  id: string
  label: string
  device_mac: string
  device_name: string | null
}

export interface EthernetSetResult {
  // Non-empty (and nothing applied) when confirmation is required; empty once applied.
  pending_published_port_deletions: AffectedPublishedPort[]
}

// SSH Keys types
export interface SshKeyFromApi {
  algorithm: string
  fingerprint: string
  hostname: string
}

export type SshKeysAddRequest = { key: string }
export type SshKeysDeleteRequest = { fingerprint: string }

export type LanIpv4Response = {
  address: string
  netmask: string
}

export type LanIpv4SetRequest = {
  address: string
  force?: boolean
}

export type LanIpv6Response = {
  slaac: boolean
  dhcpv6: boolean
  prefix: number
  ip6addr: string | null
  wan_prefix: number
}

export type LanIpv6SetRequest = {
  slaac: boolean
  dhcpv6: boolean
  prefix: number
}

export type LogEntry = { timestamp: string; message: string }
export type LogsResponse = { entries: LogEntry[] }

export type LoginReq = { password: string }

export type ExecReq = {
  command: string
  args: string[]
  timeout: number
}

export type ExecRes = {
  stdout: string
  stderr: string
  exitCode: number
}

export type GetFileReq = {
  path: string
}

export type GetFileRes = {
  contents: string
  modified: string
}

export type SetFileReq = GetFileReq & GetFileRes

export type GetUciReq = {
  names: string[]
}

export type GetUciRes<T> = T

export type SetUciReq = Record<string, UciFile<any>>

export type SetUciRes<T extends string[]> = {
  [K in T[number]]: string
}

export type SystemInfoRes = {
  version: string
  language: string
  date: string
  theme: 'dark' | 'light' | 'system'
  remoteAccess: RemoteAccess
  timezone: string
}

export type VersionInfo = {
  version: string
  releaseNotes: string
}

export type SystemUpdateReq = {
  registry?: string
  targetVersion?: string
}

export type SystemUpdateRes = {
  target: string | null
  progress: string | null
}

// Progress types for WebSocket streaming (mirrors start-os FullProgress)
export type FullProgress = {
  overall: Progress
  phases: NamedProgress[]
}

export type NamedProgress = {
  name: string
  progress: Progress
}

// null = NotStarted, boolean = Complete, object = in-progress
export type Progress =
  | null
  | boolean
  | { done: number; total: number | null; units: string | null }

export type SetPasswordReq = {
  oldPassword: string
  newPassword: string
}

export type Theme = 'dark' | 'light' | 'system'

export type RemoteAccess = 'default' | 'never' | 'always'

export type SetPreferencesReq = Partial<{
  language: string
  theme: Theme
  remoteAccess: RemoteAccess
}>

export interface VpnServerPeer {
  name: string
  ip?: string
  public_key?: string
  preshared_key?: string
  route_all?: boolean
}

export interface VpnServer {
  profile: string
  label: string
  enabled: boolean
  listen_port: number
  endpoint: string
  public_key: string
  server_address: string
  peers: VpnServerPeer[]
}

export interface VpnServerConfig {
  label: string
  enabled: boolean
  listen_port: number
  endpoint: string
  private_key?: string
}

export interface VpnServerEndpoint {
  address: string
  label: string
}

export interface VpnServers {
  servers: VpnServer[]
}

export interface VpnServerSetArgs {
  profile: string
  config: VpnServerConfig
}

export interface VpnServerDeleteArgs {
  profile: string
}

export interface VpnServerPeerAddArgs {
  profile: string
  peer: VpnServerPeer
}

export interface VpnServerPeerDeleteArgs {
  profile: string
  public_key: string
}

export interface VpnServerPeerAddResponse {
  client_config?: string
  public_key: string
  ip: string
}

export interface WifiRadio {
  band: string
  channel: string
  enabled: boolean
  broadcast: boolean
}

export interface WifiPassword {
  label: string
  profile: WifiProfileId | null
  password: string
}

export interface WifiProfileId {
  fullname: string
  interface: string
  vlan_tag: number
}

export interface WifiConfig {
  ssid: string
  broadcastSeparately: boolean
  radios: Record<string, WifiRadio>
  passwords: WifiPassword[]
  // When true, authorize deleting the published ports listed in a prior
  // WifiSetResult.pendingPublishedPortDeletions response.
  confirmPublishedPortDeletion?: boolean
}

export interface WifiSetResult {
  // Non-empty (and nothing applied) when confirmation is required; empty once applied.
  pendingPublishedPortDeletions: AffectedPublishedPort[]
}

export interface ScheduleWindow {
  startTime: string
  endTime: string
  days: [boolean, boolean, boolean, boolean, boolean, boolean, boolean]
}

// Security Profile types
export interface ProfileId {
  fullname: string
  interface: string
  vlan_tag: number
}

export interface ProfileIdOpt {
  fullname?: string
  interface?: string
  vlan_tag?: number
}

export interface SecurityProfile {
  fullname: string
  interface: string
  vlan_tag: number
  gateway_ip: string
  outbound: string // 'wan' for default WAN, or VPN interface name
  lan_access: LanAccess<ProfileId>
  wan_access: WanAccess
  access_to_new_profiles: boolean
  owns_lan: boolean
  dns_override?: DnsServer[]
  dns_source: 'system' | 'custom' | 'vpn'
}

export type LanAccess<Id = ProfileId> =
  | 'ALL'
  | 'SAME_PROFILE'
  | { other_profiles: Id[] }

export type WanAccess =
  | 'ALL'
  | 'NONE'
  | { whitelist: string[] }
  | { blacklist: string[] }

export interface ProfileCreateInput {
  fullname?: string
  interface?: string
  vlan_tag?: number
  gateway_ip: string
  outbound: string
  lan_access: LanAccess<ProfileIdOpt>
  wan_access: WanAccess
  access_to_new_profiles: boolean
  owns_lan: boolean
  dns_override?: DnsServer[]
}

export interface ProfileUpdateInput {
  fullname?: string
  interface: string
  vlan_tag: number
  gateway_ip: string
  outbound: string
  lan_access: LanAccess<ProfileIdOpt>
  wan_access: WanAccess
  access_to_new_profiles: boolean
  owns_lan: boolean
  dns_override?: DnsServer[]
  force?: boolean
}
export type CheckInitializedRes = { initialized: boolean }

export type SetInitialPasswordReq = { password: string }

export interface SetupStatusRes {
  setupMode: boolean
  disk: {
    emmcFound: boolean
    hasFirmware: boolean
  }
}

export interface SetupFlashReq {
  mode: 'update' | 'fresh-start'
  password: string
}

export interface SetupFlashEvent {
  phase: 'copying' | 'status' | 'complete' | 'error'
  copied?: number
  total?: number
  message?: string
  step?: number
  totalSteps?: number
}

// Device types (from backend smart endpoints)
export interface DeviceFromApi {
  mac: string | null
  name: string
  hostname: string | null
  status: 'online' | 'offline'
  connection: string | null
  ipv4: string | null
  ipv6: string | null
  ipv4_static: boolean
  ipv6_static: boolean
  security_profile: string | null
  speed: { up: number; down: number } | null
  data_usage: number | null
}

export interface DeviceUpdateReq {
  mac: string
  name: string
  ipv4_static: boolean
  ipv4: string
  ipv6_static: boolean
  ipv6: string
}

export type DeviceDataUsagePeriod = 'week' | 'month' | '3months'

export interface DeviceDataUsageReq {
  mac: string
  period: DeviceDataUsagePeriod
}

export interface DataUsagePointFromApi {
  timestamp: number
  upload: number
  download: number
}

// WAN types

export type WanIpv4Mode = 'dhcp' | 'static' | 'pppoe'

export type WanIpv4Response = {
  mode: WanIpv4Mode
  assigned_ip: string | null
  address: string | null
  netmask: string | null
  gateway: string | null
  username: string | null
  password: string | null
  device: string | null
}

export type WanIpv4SetRequest = {
  mode: WanIpv4Mode
  address?: string
  netmask?: string
  gateway?: string
  username?: string
  password?: string
  device?: string
}

export type WanIpv6Mode = 'disabled' | 'slaac' | 'dhcpv6' | 'static' | '6rd'

export type WanIpv6Response = {
  mode: WanIpv6Mode
  address?: string | null
  prefix?: string | null
  gateway?: string | null
  ip6prefix?: string | null
  ip6prefixlen?: string | null
  ip4prefixlen?: string | null
  border_relay?: string | null
  assigned_ipv6?: string | null
  lan_prefix?: string | null
}

export type WanIpv6SetRequest = {
  mode: WanIpv6Mode
  address?: string
  prefix?: string
  gateway?: string
  ip6prefix?: string
  ip6prefixlen?: string
  ip4prefixlen?: string
  border_relay?: string
  lan_prefix?: string
}

export type WanMacStrategy = 'router' | 'custom'

export type WanMacResponse = {
  strategy: WanMacStrategy
  mac: string
  default_mac: string
}

export type WanMacSetRequest = {
  strategy: WanMacStrategy
  mac?: string
}

export type WanDnsMode = 'isp' | 'custom'

export type DnsServer = {
  address: string
  ssl: boolean
}

export type WanDnsResponse = {
  mode: WanDnsMode
  servers: DnsServer[]
}

export type WanDnsSetRequest = {
  mode: WanDnsMode
  servers?: DnsServer[]
}

export type WanDdnsProvider =
  | 'start9'
  | 'dyndns'
  | 'noip'
  | 'cloudflare'
  | 'duckdns'
  | 'freedns'

export type WanDdnsResponse = {
  enabled: boolean
  provider: WanDdnsProvider
  hostname?: string | null
  username?: string | null
  password?: string | null
  token?: string | null
  zone?: string | null
}

export type WanDdnsSetRequest = {
  enabled: boolean
  provider: WanDdnsProvider
  hostname?: string
  username?: string
  password?: string
  token?: string
  zone?: string
}

// Published Ports types (from backend smart endpoints)
export type PublishedPortProtocol = 'tcp' | 'udp' | 'tcp+udp'
export type PublishedPortStatusValue =
  | 'active'
  | 'partial'
  | 'paused'
  | 'error'
  | 'disabled'

export interface PublishedPortFromApi {
  id: string
  enabled: boolean
  label: string
  device_mac: string
  ports: string
  protocol: PublishedPortProtocol
  ipv4: boolean
  ipv6: boolean
  ipv4_public_port: string | null
  source: string
  status: PublishedPortStatusValue
  status_reason: string | null
  device_name: string | null
  device_ipv4: string | null
  device_ipv6: string | null
}

export interface PublishedPortInputForApi {
  id: string
  enabled: boolean
  label: string
  device_mac: string
  ports: string
  protocol: PublishedPortProtocol
  ipv4: boolean
  ipv6: boolean
  ipv4_public_port?: string | null
  source: string
}

export type PublishedPortsSetRequest = {
  ports: PublishedPortInputForApi[]
}

// Outbound VPN (WireGuard Client) types

export interface OutboundVpn {
  id: string
  label: string
  target: string
  enabled: boolean
  used_by: string[]
  supports_ipv6: boolean
  /** Interface MTU, or null to inherit the kernel default (~1420). */
  mtu: number | null
}

export interface OutboundVpnCreateRequest {
  label: string
  target: string
  config: string
}

export interface OutboundVpnCreateResponse {
  id: string
}

export interface OutboundVpnUpdateRequest {
  id: string
  label: string
  target: string
  /** Desired MTU; null clears it (inherit default). Always sent by the form. */
  mtu: number | null
}

export interface OutboundVpnDeleteRequest {
  id: string
}

export interface OutboundVpnSetEnabledRequest {
  id: string
  enabled: boolean
}

// Activity types

export interface ActivityEntry {
  id: number
  timestamp: string
  category: string
  action: string
  success: boolean
  summary: string
  error: string | null
}

export interface ActivityListResponse {
  entries: ActivityEntry[]
  total: number
}

export interface ActivityListParams {
  offset?: number
  limit?: number
}
