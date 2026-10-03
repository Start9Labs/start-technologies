export type RpcHandler = {
  _PARAMS: unknown
  _RETURN?: unknown
  _CHILDREN?: { [name: string]: RpcHandler }
}

export type RpcMethod<Root extends RpcHandler> =
  | (Root extends { _RETURN: unknown } ? '' : never)
  | (Root extends { _CHILDREN: infer Children }
      ? {
          [Name in keyof Children & string]: Children[Name] extends RpcHandler
            ? RpcMethod<Children[Name]> extends infer ChildMethod extends string
              ? ChildMethod extends ''
                ? Name
                : `${Name}.${ChildMethod}`
              : never
            : never
        }[keyof Children & string]
      : never)

export type RpcParamType<
  Root extends RpcHandler,
  Method extends string,
> = Method extends ''
  ? Root extends { _RETURN: unknown }
    ? Root['_PARAMS']
    : never
  : Root extends { _CHILDREN: infer Children }
    ? Method extends `${infer Head}.${infer Tail}`
      ? Head extends keyof Children
        ? Children[Head] extends RpcHandler
          ? Root['_PARAMS'] & RpcParamType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? Root['_PARAMS'] & RpcParamType<Children[Method], ''>
          : never
        : never
    : never

export type RpcReturnType<
  Root extends RpcHandler,
  Method extends string,
> = Method extends ''
  ? Root extends { _RETURN: infer Return }
    ? Return
    : never
  : Root extends { _CHILDREN: infer Children }
    ? Method extends `${infer Head}.${infer Tail}`
      ? Head extends keyof Children
        ? Children[Head] extends RpcHandler
          ? RpcReturnType<Children[Head], Tail>
          : never
        : never
      : Method extends keyof Children
        ? Children[Method] extends RpcHandler
          ? RpcReturnType<Children[Method], ''>
          : never
        : never
    : never

export type ActivityDeleteParamsInput = { id: number }
export type ActivityEntry = {
  id: number
  timestamp: string
  category: string
  action: string
  success: boolean
  summary: string
  error?: string | null
}
export type ActivityListParamsInput = {
  offset?: number | null
  limit?: number | null
}
export type ActivityListResponse = { entries: ActivityEntry[]; total: number }
export type AffectedPublishedPort = {
  id: string
  label: string
  device_mac: string
  device_name: string | null
}
export type Api = {
  _PARAMS: {}
  _CHILDREN: {
    activity: {
      _PARAMS: {}
      _CHILDREN: {
        clear: { _PARAMS: {}; _RETURN: null }
        delete: { _PARAMS: ActivityDeleteParamsInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            ActivityListParamsInput
          _RETURN: ActivityListResponse
        }
      }
    }
    auth: {
      _PARAMS: {}
      _CHILDREN: {
        'check-initialized': { _PARAMS: {}; _RETURN: CheckInitializedRes }
        login: { _PARAMS: LoginParamsInput; _RETURN: LoginRes }
        logout: { _PARAMS: LogoutParamsInput; _RETURN: null }
        'set-initial-password': {
          _PARAMS: SetInitialPasswordParamsInput
          _RETURN: LoginRes
        }
        'set-password': { _PARAMS: ResetPasswordParamsInput; _RETURN: null }
        'verify-password': { _PARAMS: VerifyPasswordParamsInput; _RETURN: null }
      }
    }
    backup: {
      _PARAMS: {}
      _CHILDREN: {
        create: { _PARAMS: {}; _RETURN: BackupCreateRes }
        restore: { _PARAMS: {}; _RETURN: BackupRestoreRes }
      }
    }
    devices: {
      _PARAMS: {}
      _CHILDREN: {
        'data-usage': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & DataUsageReqInput
          _RETURN: DataUsagePoint[]
        }
        forget: { _PARAMS: DeviceMacReqInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: Device[]
        }
        'set-auto-forward': {
          _PARAMS: SetAutoForwardRequestInput
          _RETURN: null
        }
        update: { _PARAMS: DeviceUpdateReqInput; _RETURN: null }
      }
    }
    diagnostics: {
      _PARAMS: {}
      _CHILDREN: { create: { _PARAMS: {}; _RETURN: DiagnosticsCreateRes } }
    }
    dir: {
      _PARAMS: {}
      _CHILDREN: {
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & DirGetArgsInput
          _RETURN: DirEntry[]
        }
      }
    }
    ethernet: {
      _PARAMS: {}
      _CHILDREN: {
        edit: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: null
        }
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: {
            wan_ipv6: boolean
            wan_port: string | null
            ports: { [key: string]: { profile: ProfileId | null } }
          }
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            EthernetSetRequestInput
          _RETURN: EthernetSetResult
        }
      }
    }
    exec: {
      _PARAMS: ({ format?: IoFormatInput | null } & {}) & ExecReqInput
      _RETURN: ExecRes
    }
    file: {
      _PARAMS: {}
      _CHILDREN: {
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & GetFileArgsInput
          _RETURN: FileContents
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & SetFileArgsInput
          _RETURN: null
        }
      }
    }
    lan: {
      _PARAMS: {}
      _CHILDREN: {
        'ipv4-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: LanIpv4Response
        }
        'ipv4-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            LanIpv4SetRequestInput
          _RETURN: null
        }
        'ipv6-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: LanIpv6Response
        }
        'ipv6-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            LanIpv6SetRequestInput
          _RETURN: null
        }
      }
    }
    profiles: {
      _PARAMS: {}
      _CHILDREN: {
        create: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            ({
              gateway_ip: string
              outbound: string
              lan_access:
                | 'ALL'
                | 'SAME_PROFILE'
                | { other_profiles: ProfileIdOptInput[] }
              wan_access: WanAccessInput
              dns_override?: DnsServerInput[]
              dns_source?: string
              access_to_new_profiles: boolean
              owns_lan: boolean
            } & ProfileIdOptInput)
          _RETURN: ProfileId
        }
        delete: { _PARAMS: ProfileIdOptInput; _RETURN: null }
        edit: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & EditArgsInput
          _RETURN: ProfileId
        }
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & ProfileIdOptInput
          _RETURN: {
            gateway_ip: string
            outbound: string
            lan_access: 'ALL' | 'SAME_PROFILE' | { other_profiles: ProfileId[] }
            wan_access: WanAccess
            dns_override: DnsServer[]
            dns_source: string
            access_to_new_profiles: boolean
            owns_lan: boolean
          } & ProfileId
        }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: ProfileId[]
        }
        'schedule-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            ScheduleGetParamsInput
          _RETURN: ScheduleWindow[]
        }
        'schedule-set': { _PARAMS: ScheduleWindowsInput; _RETURN: null }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            ProfileSetRequestInput
          _RETURN: ProfileId
        }
      }
    }
    'published-ports': {
      _PARAMS: {}
      _CHILDREN: {
        'auto-list': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: AutomaticPortUse[]
        }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: PublishedPort[]
        }
        reconcile: { _PARAMS: {}; _RETURN: unknown }
        set: {
          _PARAMS: PublishedPortsSetRequestInput
          _RETURN: PublishedPortsSetResult
        }
        'sync-hairpin': { _PARAMS: {}; _RETURN: unknown }
        'wan-changed': { _PARAMS: {}; _RETURN: unknown }
      }
    }
    setup: {
      _PARAMS: {}
      _CHILDREN: { status: { _PARAMS: {}; _RETURN: SetupStatusRes } }
    }
    'ssh-keys': {
      _PARAMS: {}
      _CHILDREN: {
        add: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            SshKeyAddParamsInput
          _RETURN: SshKeyResponse
        }
        delete: { _PARAMS: SshKeyDeleteParamsInput; _RETURN: null }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: SshKeyResponse[]
        }
      }
    }
    system: {
      _PARAMS: {}
      _CHILDREN: {
        'apply-remote-access': { _PARAMS: {}; _RETURN: unknown }
        'factory-reset': { _PARAMS: {}; _RETURN: null }
        'get-timezones': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: string[]
        }
        info: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: SystemInfoResponse
        }
        logs: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: LogsResponse
        }
        'newer-versions': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: VersionInfo[]
        }
        restart: { _PARAMS: {}; _RETURN: null }
        'set-preferences': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            SetPreferencesReqInput
          _RETURN: unknown
        }
        'set-timezone': { _PARAMS: SetTimezoneParamsInput; _RETURN: null }
        update: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            UpdateSystemParamsInput
          _RETURN: UpdateSystemRes
        }
      }
    }
    uci: {
      _PARAMS: {}
      _CHILDREN: {
        edit: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & GetArgsInput
          _RETURN: { [key: string]: string }
        }
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & GetArgsInput
          _RETURN: { [key: string]: UciFile }
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {
            [key: string]: UciFileInput
          }
          _RETURN: { [key: string]: string }
        }
      }
    }
    'vpn-client': {
      _PARAMS: {}
      _CHILDREN: {
        create: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            OutboundVpnCreateRequestInput
          _RETURN: OutboundVpnCreateResponse
        }
        delete: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            OutboundVpnDeleteRequestInput
          _RETURN: null
        }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: OutboundVpn[]
        }
        'set-enabled': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            OutboundVpnSetEnabledRequestInput
          _RETURN: null
        }
        update: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            OutboundVpnUpdateRequestInput
          _RETURN: null
        }
      }
    }
    'vpn-server': {
      _PARAMS: {}
      _CHILDREN: {
        delete: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & DeleteArgsInput
          _RETURN: null
        }
        list: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: VpnServers
        }
        'peer-add': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & PeerAddArgsInput
          _RETURN: PeerAddResponse
        }
        'peer-delete': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            PeerDeleteArgsInput
          _RETURN: null
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & SetArgsInput
          _RETURN: null
        }
      }
    }
    wan: {
      _PARAMS: {}
      _CHILDREN: {
        'ddns-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WanDdnsResponse
        }
        'ddns-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            WanDdnsSetRequestInput
          _RETURN: null
        }
        'dns-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WanDnsResponse
        }
        'dns-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            WanDnsSetRequestInput
          _RETURN: null
        }
        'ipv4-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WanIpv4Response
        }
        'ipv4-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            WanIpv4SetRequestInput
          _RETURN: null
        }
        'ipv6-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WanIpv6Response
        }
        'ipv6-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            WanIpv6SetRequestInput
          _RETURN: null
        }
        'mac-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WanMacResponse
        }
        'mac-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            WanMacSetRequestInput
          _RETURN: null
        }
      }
    }
    wifi: {
      _PARAMS: {}
      _CHILDREN: {
        'blackout-get': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: BlackoutWindow[]
        }
        'blackout-set': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            BlackoutWindowsInput
          _RETURN: null
        }
        edit: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: null
        }
        'generate-password': {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: string
        }
        get: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: {
            ssid: string
            broadcastSeparately: boolean
            country: string | null
            radios: { [key: string]: WifiRadio }
            passwords: {
              label: string
              profile: ProfileId | null
              password: string
            }[]
          }
        }
        regulatory: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) & {}
          _RETURN: WifiRegulatory
        }
        set: {
          _PARAMS: ({ format?: IoFormatInput | null } & {}) &
            WifiSetRequestInput
          _RETURN: WifiSetResult
        }
      }
    }
  }
}
export type AutomaticPortUse = {
  id: string
  kind: string
  device_mac: string
  device_name: string | null
  internal_ip: string | null
  ports: string
  public_ports: string
  expires_secs: number | null
  hostname: string | null
}
export type BackupCreateRes = { guid: Guid; filename: string }
export type BackupRestoreRes = { upload: Guid }
export type BlackoutWindow = {
  startTime: string
  endTime: string
  days: [boolean, boolean, boolean, boolean, boolean, boolean, boolean]
}
export type BlackoutWindowInput = {
  startTime: string
  endTime: string
  days: [boolean, boolean, boolean, boolean, boolean, boolean, boolean]
}
export type BlackoutWindowsInput = { windows: BlackoutWindowInput[] }
export type CheckInitializedRes = { initialized: boolean }
export type DataUsagePeriodInput = 'week' | 'month' | '3months'
export type DataUsagePoint = {
  timestamp: number
  upload: number
  download: number
}
export type DataUsageReqInput = { mac: string; period: DataUsagePeriodInput }
export type DdnsProvider =
  | 'dyndns'
  | 'noip'
  | 'cloudflare'
  | 'duckdns'
  | 'freedns'
export type DdnsProviderInput =
  | 'dyndns'
  | 'noip'
  | 'cloudflare'
  | 'duckdns'
  | 'freedns'
export type DeleteArgsInput = { profile: string }
export type Device = {
  mac: string | null
  name: string
  custom_name: string | null
  hostname: string | null
  status: DeviceStatus
  connection: string | null
  ipv4: string | null
  ipv6: string | null
  ipv4_static: boolean
  allow_auto_port_forward: boolean
  security_profile: string | null
  speed: SpeedData | null
  data_usage: (number | null) | null
}
export type DeviceMacReqInput = { mac: string }
export type DeviceStatus = 'online' | 'offline'
export type DeviceUpdateReqInput = {
  mac: string
  name?: string | null
  ipv4_static: boolean
  ipv4: string
}
export type DiagnosticsCreateRes = { guid: Guid; filename: string }
export type DirEntry = {
  name: string
  size: number
  blocks: number
  io_block: number
  file_type: FileType
  device: number
  inode: number
  links: number
  mode: number
  uid: number
  gid: number
  access: string
  modify: string
  change: string
}
export type DirGetArgsInput = { path: string }
export type DiskState = { emmcFound: boolean; hasFirmware: boolean }
export type DnsMode = 'isp' | 'custom'
export type DnsModeInput = 'isp' | 'custom'
export type DnsServer = { address: string; ssl: boolean }
export type DnsServerInput = { address: string; ssl: boolean }
export type EditArgsInput = { get: ProfileIdOptInput; create: boolean }
export type EthernetSetRequestInput = {
  confirm_published_port_deletion?: boolean
} & {
  wan_ipv6: boolean
  wan_port?: string | null
  ports: { [key: string]: { profile?: ProfileIdOptInput | null } }
}
export type EthernetSetResult = {
  pending_published_port_deletions: AffectedPublishedPort[]
}
export type ExecReqInput = { command: string; args: string[]; timeout: number }
export type ExecRes = { stdout: string; stderr: string; exitCode: number }
export type FileContents = { contents: string; modified: string }
export type FileType =
  | 'regular-file'
  | 'directory'
  | 'symlink'
  | 'block-device'
  | 'char-device'
  | 'fifo'
  | 'socket'
export type GetArgsInput = { names: string[] }
export type GetFileArgsInput = { path: string }
export type Guid = string
export type IoFormatInput =
  | 'json'
  | 'json-pretty'
  | 'yaml'
  | 'cbor'
  | 'toml'
  | 'toml-pretty'
export type LanIpv4Response = { address: string; netmask: string }
export type LanIpv4SetRequestInput = { address: string; force?: boolean }
export type LanIpv6Response = {
  slaac: boolean
  dhcpv6: boolean
  prefix: number
  ip6addr: string | null
  wan_prefix: number
}
export type LanIpv6SetRequestInput = {
  slaac: boolean
  dhcpv6: boolean
  prefix: number
}
export type LogEntry = { timestamp: string; message: string }
export type LoginParamsInput = { password: string; userAgent?: string | null }
export type LoginRes = { session: string }
export type LogoutParamsInput = { sessionHash?: string | null }
export type LogsResponse = { entries: LogEntry[] }
export type MacStrategy = 'router' | 'custom'
export type MacStrategyInput = 'router' | 'custom'
export type OutboundVpn = {
  id: string
  label: string
  target: string
  enabled: boolean
  used_by: string[]
  supports_ipv6: boolean
  mtu: number | null
  hostname_endpoint: boolean
}
export type OutboundVpnCreateRequestInput = {
  label: string
  target: string
  config: string
}
export type OutboundVpnCreateResponse = { id: string }
export type OutboundVpnDeleteRequestInput = { id: string }
export type OutboundVpnSetEnabledRequestInput = { id: string; enabled: boolean }
export type OutboundVpnUpdateRequestInput = {
  id: string
  label: string
  target: string
  mtu?: number | null
}
export type PeerAddArgsInput = { profile: string; peer: VpnServerPeerInput }
export type PeerAddResponse = {
  client_config?: string | null
  public_key: string
  ip: string
}
export type PeerDeleteArgsInput = { profile: string; public_key: string }
export type ProfileId = {
  fullname: string
  interface: string
  vlan_tag: number
}
export type ProfileIdOptInput = {
  fullname?: string | null
  interface?: string | null
  vlan_tag?: number | null
}
export type ProfileSetRequestInput = { force?: boolean } & ({
  gateway_ip: string
  outbound: string
  lan_access: 'ALL' | 'SAME_PROFILE' | { other_profiles: ProfileIdOptInput[] }
  wan_access: WanAccessInput
  dns_override?: DnsServerInput[]
  dns_source?: string
  access_to_new_profiles: boolean
  owns_lan: boolean
} & ProfileIdOptInput)
export type Protocol = 'tcp' | 'udp' | 'tcp+udp'
export type ProtocolInput = 'tcp' | 'udp' | 'tcp+udp'
export type PublishedPort = {
  id: string
  enabled: boolean
  label: string
  device_mac: string
  ports: string
  protocol: Protocol
  ipv4: boolean
  ipv6: boolean
  ipv4_public_port: string | null
  source: string
  override_wan_ports: boolean
  status: PublishedPortStatus
  status_reason: string | null
  device_name: string | null
  device_ipv4: string | null
  device_ipv6: string | null
}
export type PublishedPortInputInput = {
  id: string
  enabled: boolean
  label: string
  device_mac: string
  ports: string
  protocol: ProtocolInput
  ipv4: boolean
  ipv6: boolean
  ipv4_public_port?: string | null
  source: string
  override_wan_ports?: boolean
}
export type PublishedPortStatus =
  | 'active'
  | 'partial'
  | 'paused'
  | 'error'
  | 'disabled'
export type PublishedPortsSetRequestInput = { ports: PublishedPortInputInput[] }
export type PublishedPortsSetResult = {
  pending_wan_port_collisions: WanPortCollision[]
}
export type ResetPasswordParamsInput = {
  oldPassword: string
  newPassword: string
}
export type ScheduleGetParamsInput = { interface: string }
export type ScheduleWindow = {
  startTime: string
  endTime: string
  days: [boolean, boolean, boolean, boolean, boolean, boolean, boolean]
}
export type ScheduleWindowInput = {
  startTime: string
  endTime: string
  days: [boolean, boolean, boolean, boolean, boolean, boolean, boolean]
}
export type ScheduleWindowsInput = {
  interface: string
  windows: ScheduleWindowInput[]
}
export type Section = {
  ty: string
  name: string | null
  options: { [key: string]: string }
  lists: { [key: string]: string[] }
}
export type SectionInput = {
  ty: string
  name?: string | null
  options: { [key: string]: string }
  lists: { [key: string]: string[] }
}
export type SetArgsInput = { profile: string; config: VpnServerConfigInput }
export type SetAutoForwardRequestInput = { mac: string; allow: boolean }
export type SetFileArgsInput = {
  path: string
  contents: string
  modified?: string | null
}
export type SetInitialPasswordParamsInput = { password: string }
export type SetPreferencesReqInput = {
  language?: string | null
  theme?: string | null
  remoteAccess?: string | null
}
export type SetTimezoneParamsInput = { timezone: string }
export type SetupStatusRes = { setupMode: boolean; disk: DiskState }
export type SniPortUse = {
  ports: string
  hostnames: string[]
  devices: string[]
}
export type SpeedData = { up: number | null; down: number | null }
export type SshKeyAddParamsInput = { key: string }
export type SshKeyDeleteParamsInput = { fingerprint: string }
export type SshKeyResponse = {
  algorithm: string
  fingerprint: string
  hostname: string
}
export type SystemInfoResponse = {
  version: string
  gitHash: string
  language: string
  date: string
  theme: string
  remoteAccess: string
  timezone: string
}
export type UciFile = { sections: Section[]; modified: string | null }
export type UciFileInput = {
  sections: SectionInput[]
  modified?: string | null
}
export type UpdateSystemParamsInput = {
  registry?: string | null
  targetVersion?: string | null
}
export type UpdateSystemRes = { target: string | null; progress: string | null }
export type VerifyPasswordParamsInput = { password: string }
export type VersionInfo = { version: string; releaseNotes: string }
export type VpnServer = {
  profile: string
  label: string
  enabled: boolean
  listen_port: number
  endpoint: string
  public_key: string
  server_address: string
  peers: VpnServerPeer[]
}
export type VpnServerConfigInput = {
  label: string
  enabled: boolean
  listen_port: number
  endpoint: string
  private_key?: string | null
}
export type VpnServerPeer = {
  name: string
  ip?: string | null
  public_key?: string | null
  preshared_key?: string | null
  route_all?: boolean | null
}
export type VpnServerPeerInput = {
  name: string
  ip?: string | null
  public_key?: string | null
  preshared_key?: string | null
  route_all?: boolean | null
}
export type VpnServers = { servers: VpnServer[] }
export type WanAccess =
  | 'ALL'
  | 'NONE'
  | { whitelist: string[] }
  | { blacklist: string[] }
export type WanAccessInput =
  | 'ALL'
  | 'NONE'
  | { whitelist: string[] }
  | { blacklist: string[] }
export type WanDdnsResponse = {
  enabled: boolean
  provider: DdnsProvider
  hostname: string | null
  username: string | null
  password: string | null
  token: string | null
  zone: string | null
}
export type WanDdnsSetRequestInput = {
  enabled: boolean
  provider: DdnsProviderInput
  hostname?: string | null
  username?: string | null
  password?: string | null
  token?: string | null
  zone?: string | null
}
export type WanDnsResponse = { mode: DnsMode; servers: DnsServer[] }
export type WanDnsSetRequestInput = {
  mode: DnsModeInput
  servers?: DnsServerInput[] | null
}
export type WanIpv4Mode = 'dhcp' | 'static' | 'pppoe'
export type WanIpv4ModeInput = 'dhcp' | 'static' | 'pppoe'
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
export type WanIpv4SetRequestInput = {
  mode: WanIpv4ModeInput
  address?: string | null
  netmask?: string | null
  gateway?: string | null
  username?: string | null
  password?: string | null
  device?: string | null
}
export type WanIpv6Mode = 'disabled' | 'slaac' | 'dhcpv6' | 'static' | '6rd'
export type WanIpv6ModeInput =
  | 'disabled'
  | 'slaac'
  | 'dhcpv6'
  | 'static'
  | '6rd'
export type WanIpv6Response = {
  mode: WanIpv6Mode
  address: string | null
  prefix: string | null
  gateway: string | null
  ip6prefix: string | null
  ip6prefixlen: string | null
  ip4prefixlen: string | null
  border_relay: string | null
  assigned_ipv6: string | null
  lan_prefix: string | null
}
export type WanIpv6SetRequestInput = {
  mode: WanIpv6ModeInput
  address?: string | null
  prefix?: string | null
  gateway?: string | null
  ip6prefix?: string | null
  ip6prefixlen?: string | null
  ip4prefixlen?: string | null
  border_relay?: string | null
  lan_prefix?: string | null
}
export type WanMacResponse = {
  strategy: MacStrategy
  mac: string
  default_mac: string
}
export type WanMacSetRequestInput = {
  strategy: MacStrategyInput
  mac?: string | null
}
export type WanPortCollision = {
  id: string
  label: string
  router_service_ports: string[]
  hostname_route_ports: SniPortUse[]
}
export type WifiRadio = {
  band: string
  channel: string
  enabled: boolean
  broadcast: boolean
}
export type WifiRadioInput = {
  band: string
  channel: string
  enabled: boolean
  broadcast: boolean
}
export type WifiRegulatory = {
  countries: string[]
  channels: { [key: string]: number[] }
}
export type WifiSetRequestInput = { confirmPublishedPortDeletion?: boolean } & {
  ssid: string
  broadcastSeparately: boolean
  country?: string | null
  radios: { [key: string]: WifiRadioInput }
  passwords: {
    label: string
    profile?: ProfileIdOptInput | null
    password: string
  }[]
}
export type WifiSetResult = {
  pendingPublishedPortDeletions: AffectedPublishedPort[]
}
