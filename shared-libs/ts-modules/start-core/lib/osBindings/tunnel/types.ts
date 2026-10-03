export type AddDeviceParams = {
  subnet: string
  name: string
  ip: string | null
  kind: WgClientKind
}
export type AddDeviceParamsInput = {
  subnet: string
  name: string
  ip?: string | null
  kind?: WgClientKindInput
}
export type AddDnsRecordParams = {
  name: string
  type: string
  value: string
  ttl: number | null
}
export type AddDnsRecordParamsInput = {
  name: string
  type: string
  value: string
  ttl?: number | null
}
export type AddKeyParams = { name: string; key: AnyVerifyingKey }
export type AddKeyParamsInput = { name: string; key: AnyVerifyingKeyInput }
export type AddPinholeParams = {
  gua: string
  externalPort: number
  internalPort: number | null
  label: string | null
  count: number | null
}
export type AddPinholeParamsInput = {
  gua: string
  externalPort: number
  internalPort?: number | null
  label?: string | null
  count?: number | null
}
export type AddPortForwardParams = {
  externalPort: number
  target: string
  label: string | null
  sni: string[]
  count: number | null
}
export type AddPortForwardParamsInput = {
  externalPort: number
  target: string
  label?: string | null
  sni?: string[]
  count?: number | null
}
export type AddSubnetParams = { name: string }
export type AddSubnetParamsInput = { name: string }
export type AnyVerifyingKey = string
export type AnyVerifyingKeyInput = string
export type AuthKeys = { [key: string]: Session }
export type AuthKeysInput = { [key: string]: SessionInput }
export type Base64 = string
export type Base64Input = string
export type CapabilityVerdict = { supported: boolean | null; at: string | null }
export type CapabilityVerdictInput = {
  supported?: boolean | null
  at?: string | null
}
export type DnsConfig =
  | { type: 'default' }
  | ({ type: 'device' } & { ip: string })
  | ({ type: 'custom' } & { servers: string[] })
export type DnsConfigInput =
  | { type: 'default' }
  | ({ type: 'device' } & { ip: string })
  | ({ type: 'custom' } & { servers: string[] })
export type DnsMode = 'default' | 'device' | 'custom'
export type DnsModeInput = 'default' | 'device' | 'custom'
export type DnsRecordEntry = {
  name: string
  type: string
  value: string
  ttl: number
  source: string | null
}
export type DnsRecordEntryInput = {
  name: string
  type: string
  value: string
  ttl: number
  source?: string | null
}
export type DnsRecords = DnsRecordEntry[]
export type DnsRecordsInput = DnsRecordEntryInput[]
export type GatewayId = string
export type GatewayIdInput = string
export type GatewayPortMapCapabilities = {
  pcp: CapabilityVerdict
  natPmp: CapabilityVerdict
  upnp: CapabilityVerdict
  pcpHostname: CapabilityVerdict
}
export type GatewayPortMapCapabilitiesInput = {
  pcp: CapabilityVerdictInput
  natPmp: CapabilityVerdictInput
  upnp: CapabilityVerdictInput
  pcpHostname: CapabilityVerdictInput
}
export type GatewayType = 'inbound-outbound' | 'outbound-only'
export type GatewayTypeInput = 'inbound-outbound' | 'outbound-only'
export type HttpRedirectStatus = {
  ip: string
  enabled: boolean
  forwarded: boolean
}
export type HttpRedirectStatusInput = {
  ip: string
  enabled: boolean
  forwarded: boolean
}
export type HttpRedirects = { disabled: string[] }
export type HttpRedirectsInput = { disabled?: string[] }
export type IpInfo = {
  name: string
  scopeId: number
  deviceType: NetworkInterfaceType | null
  subnets: string[]
  lanIp: string[]
  wanIp: string | null
  ntpServers: string[]
  dnsServers: string[]
}
export type IpInfoInput = {
  name: string
  scopeId: number
  deviceType?: NetworkInterfaceTypeInput | null
  subnets: string[]
  lanIp: string[]
  wanIp?: string | null
  ntpServers: string[]
  dnsServers: string[]
}
export type ListDevicesParams = { subnet: string }
export type ListDevicesParamsInput = { subnet: string }
export type NetworkInterfaceInfo = {
  name: string | null
  secure: boolean | null
  ipInfo: IpInfo | null
  type: GatewayType
  portMap: GatewayPortMapCapabilities
  dnsUpdate: CapabilityVerdict
}
export type NetworkInterfaceInfoInput = {
  name?: string | null
  secure?: boolean | null
  ipInfo?: IpInfoInput | null
  type?: GatewayTypeInput | null
  portMap?: GatewayPortMapCapabilitiesInput
  dnsUpdate?: CapabilityVerdictInput
}
export type NetworkInterfaceType =
  | 'ethernet'
  | 'wireless'
  | 'bridge'
  | 'wireguard'
  | 'loopback'
export type NetworkInterfaceTypeInput =
  | 'ethernet'
  | 'wireless'
  | 'bridge'
  | 'wireguard'
  | 'loopback'
export type Pem = string
export type PemInput = string
export type Pinhole = {
  label: string | null
  enabled: boolean
  count: number
  internalPort: number | null
  auto: boolean
}
export type PinholeInput = {
  label?: string | null
  enabled?: boolean
  count?: number
  internalPort?: number | null
  auto?: boolean
}
export type Pinholes6 = { [key: string]: Pinhole }
export type Pinholes6Input = { [key: string]: PinholeInput }
export type PortForward =
  | ({ kind: 'dnat' } & {
      target: string
      label: string | null
      enabled: boolean
      count: number
      auto: boolean
    })
  | ({ kind: 'sni' } & {
      routes: { [key: string]: SniRoute }
      fallback: SniRoute | null
    })
export type PortForwardInput =
  | ({ kind: 'dnat' } & {
      target: string
      label?: string | null
      enabled?: boolean
      count?: number
      auto?: boolean
    })
  | ({ kind: 'sni' } & {
      routes: { [key: string]: SniRouteInput }
      fallback?: SniRouteInput | null
    })
export type PortForwards = { [key: string]: PortForward }
export type PortForwardsInput = { [key: string]: PortForwardInput }
export type RemoveDeviceParams = { subnet: string; ip: string }
export type RemoveDeviceParamsInput = { subnet: string; ip: string }
export type RemoveDnsRecordParams = { name: string; type: string | null }
export type RemoveDnsRecordParamsInput = { name: string; type?: string | null }
export type RemoveKeyParams = { key: AnyVerifyingKey }
export type RemoveKeyParamsInput = { key: AnyVerifyingKeyInput }
export type RemovePinholeParams = { gua: string; externalPort: number }
export type RemovePinholeParamsInput = { gua: string; externalPort: number }
export type RemovePortForwardParams = {
  source: string
  hostname: string | null
}
export type RemovePortForwardParamsInput = {
  source: string
  hostname?: string | null
}
export type Session = {
  name: string | null
  loggedIn: string
  lastActive: string
  userAgent: string | null
}
export type SessionInput = {
  name?: string | null
  loggedIn?: string
  lastActive?: string
  userAgent?: string | null
}
export type SetAutoPortForwardParams = {
  subnet: string
  ip: string
  enabled: boolean
}
export type SetAutoPortForwardParamsInput = {
  subnet: string
  ip: string
  enabled: boolean
}
export type SetDeviceKindParams = {
  subnet: string
  ip: string
  kind: WgClientKind
}
export type SetDeviceKindParamsInput = {
  subnet: string
  ip: string
  kind: WgClientKindInput
}
export type SetDeviceWanParams = {
  subnet: string
  ip: string
  wanIp: string | null
}
export type SetDeviceWanParamsInput = {
  subnet: string
  ip: string
  wanIp?: string | null
}
export type SetDnsInjectionParams = {
  subnet: string
  ip: string
  enabled: boolean
}
export type SetDnsInjectionParamsInput = {
  subnet: string
  ip: string
  enabled: boolean
}
export type SetHttpRedirectEnabledParams = { ip: string; enabled: boolean }
export type SetHttpRedirectEnabledParamsInput = { ip: string; enabled: boolean }
export type SetPasswordParams = { password: string }
export type SetPasswordParamsInput = { password: string }
export type SetPinholeEnabledParams = {
  gua: string
  externalPort: number
  enabled: boolean
}
export type SetPinholeEnabledParamsInput = {
  gua: string
  externalPort: number
  enabled: boolean
}
export type SetPortForwardEnabledParams = {
  source: string
  enabled: boolean
  hostname: string | null
}
export type SetPortForwardEnabledParamsInput = {
  source: string
  enabled: boolean
  hostname?: string | null
}
export type SetSubnetDnsParams = {
  mode: DnsMode
  deviceIp: string | null
  servers: string[]
}
export type SetSubnetDnsParamsInput = {
  mode: DnsModeInput
  deviceIp?: string | null
  servers: string[]
}
export type SetSubnetIpv6Params = { prefix: string | null }
export type SetSubnetIpv6ParamsInput = { prefix?: string | null }
export type SetSubnetWanParams = { wanIp: string | null }
export type SetSubnetWanParamsInput = { wanIp?: string | null }
export type ShowConfigParams = { subnet: string; ip: string }
export type ShowConfigParamsInput = { subnet: string; ip: string }
export type SniRoute = {
  target: string
  label: string | null
  enabled: boolean
  auto: boolean
}
export type SniRouteInput = {
  target: string
  label?: string | null
  enabled?: boolean
  auto?: boolean
}
export type SubnetParams = { subnet: string }
export type SubnetParamsInput = { subnet: string }
export type TunnelCertData = { key: string; cert: string }
export type TunnelCertDataInput = { key: string; cert: string }
export type TunnelDatabase = {
  webserver: WebserverInfo
  password: string | null
  sessionPubkeys: AuthKeys
  gateways: { [key: string]: NetworkInterfaceInfo }
  wg: WgServer
  portForwards: PortForwards
  pinholes6: Pinholes6
  dnsRecords: DnsRecords
  httpRedirects: HttpRedirects
}
export type TunnelDatabaseInput = {
  webserver: WebserverInfoInput
  password?: string | null
  sessionPubkeys?: AuthKeysInput
  gateways: { [key: string]: NetworkInterfaceInfoInput }
  wg: WgServerInput
  portForwards: PortForwardsInput
  pinholes6?: Pinholes6Input
  dnsRecords?: DnsRecordsInput
  httpRedirects?: HttpRedirectsInput
}
export type TunnelUpdateResult = {
  status: string
  installed: string
  candidate: string
}
export type TunnelUpdateResultInput = {
  status: string
  installed: string
  candidate: string
}
export type UpdatePinholeLabelParams = {
  gua: string
  externalPort: number
  label: string | null
}
export type UpdatePinholeLabelParamsInput = {
  gua: string
  externalPort: number
  label?: string | null
}
export type UpdatePortForwardLabelParams = {
  source: string
  label: string | null
  hostname: string | null
}
export type UpdatePortForwardLabelParamsInput = {
  source: string
  label?: string | null
  hostname?: string | null
}
export type WebserverInfo = {
  enabled: boolean
  listen: string | null
  certificate: TunnelCertData | null
}
export type WebserverInfoInput = {
  enabled: boolean
  listen?: string | null
  certificate?: TunnelCertDataInput | null
}
export type WgClientKind = 'client' | 'server'
export type WgClientKindInput = 'client' | 'server'
export type WgConfig = {
  name: string
  key: string
  psk: string
  kind: WgClientKind
  allowDnsInjection: boolean
  allowAutoPortForward: boolean
  wanIp: string | null
}
export type WgConfigInput = {
  name: string
  key: string
  psk: string
  kind?: WgClientKindInput
  allowDnsInjection?: boolean
  allowAutoPortForward?: boolean
  wanIp?: string | null
}
export type WgServer = { port: number; key: string; subnets: WgSubnetMap }
export type WgServerInput = {
  port: number
  key: string
  subnets: WgSubnetMapInput
}
export type WgSubnetClients = { [key: string]: WgConfig }
export type WgSubnetClientsInput = { [key: string]: WgConfigInput }
export type WgSubnetConfig = {
  name: string
  clients: WgSubnetClients
  dns: DnsConfig
  wanIp: string | null
  ipv6: string | null
}
export type WgSubnetConfigInput = {
  name: string
  clients: WgSubnetClientsInput
  dns?: DnsConfigInput
  wanIp?: string | null
  ipv6?: string | null
}
export type WgSubnetMap = { [key: string]: WgSubnetConfig }
export type WgSubnetMapInput = { [key: string]: WgSubnetConfigInput }
