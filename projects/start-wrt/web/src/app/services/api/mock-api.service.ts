import { Injectable } from '@angular/core'
import { pauseFor } from '@start9labs/shared'
import {
  ApiService,
  ExecReq,
  ExecRes,
  GetFileReq,
  GetFileRes,
  GetUciReq,
  LoginReq,
  SystemInfoRes,
  SetFileReq,
  SetUciReq,
  SetUciRes,
  VersionInfo,
  SystemUpdateReq,
  SystemUpdateRes,
  SetPasswordReq,
  SetPreferencesReq,
  VpnServer,
  VpnServerDeleteArgs,
  VpnServerPeerAddArgs,
  VpnServerPeerAddResponse,
  VpnServerPeerDeleteArgs,
  VpnServerSetArgs,
  VpnServers,
  WifiConfig,
  ScheduleWindow,
  ProfileId,
  ProfileIdOpt,
  SecurityProfile,
  ProfileCreateInput,
  ProfileUpdateInput,
  CheckInitializedRes,
  SetInitialPasswordReq,
  SetupStatusRes,
  LogEntry,
  LogsResponse,
  DeviceFromApi,
  DeviceUpdateReq,
  DeviceDataUsageReq,
  DataUsagePointFromApi,
  LanIpv4Response,
  LanIpv4SetRequest,
  LanIpv6Response,
  LanIpv6SetRequest,
  WanIpv4Response,
  WanIpv4SetRequest,
  WanIpv6Response,
  WanIpv6SetRequest,
  WanMacResponse,
  WanMacSetRequest,
  WanDnsResponse,
  WanDnsSetRequest,
  WanDdnsResponse,
  WanDdnsSetRequest,
  PublishedPortFromApi,
  PublishedPortsSetRequest,
  OutboundVpn,
  OutboundVpnCreateRequest,
  OutboundVpnCreateResponse,
  OutboundVpnUpdateRequest,
  OutboundVpnDeleteRequest,
  OutboundVpnSetEnabledRequest,
  EthernetConfig,
  EthernetSetConfig,
  EthernetSetResult,
  WifiSetResult,
  SshKeyFromApi,
  SshKeysAddRequest,
  SshKeysDeleteRequest,
  ActivityEntry,
  ActivityListParams,
  ActivityListResponse,
  BackupCreateRes,
  BackupRestoreRes,
  DiagnosticsCreateRes,
} from './api.service'
import { UciFile, UciSection } from './types'
import { dhcpLanSlaacDhcpv6 } from 'src/app/routes/lan/routes/ipv6/uci/mocks'
import {
  generateMockDataUsage,
  getMockArpOutput,
  MOCK_DEVICE_DEFS,
  mockDhcpHosts,
  mockDhcpLeasesOutput,
} from 'src/app/routes/devices/uci/mocks'

@Injectable({
  providedIn: 'root',
})
export class MockApiService extends ApiService {
  async login(params: LoginReq): Promise<null> {
    await pauseFor(250)

    return null
  }

  async logout(): Promise<null> {
    await pauseFor(250)

    return null
  }

  async setTimezone(params: { timezone: string }): Promise<null> {
    this.mockSystemInfo.timezone = params.timezone
    return null
  }

  async getTimezones(): Promise<string[]> {
    return [
      'UTC',
      'America/Chicago',
      'America/Denver',
      'America/Los_Angeles',
      'America/New_York',
      'Europe/London',
      'Europe/Paris',
    ]
  }

  async exec(params: ExecReq): Promise<ExecRes> {
    await pauseFor(250)

    // Handle specific commands for device discovery
    if (params.command === 'ip' && params.args[0] === 'neigh') {
      // Check WAN and LAN IPv6 status to determine which addresses to show
      const wanIpv6Enabled = this.isWanIpv6Enabled()
      const lanIpv6Enabled = this.isLanIpv6Enabled()

      return {
        exitCode: 0,
        stdout: getMockArpOutput(wanIpv6Enabled, lanIpv6Enabled),
        stderr: '',
      }
    }

    if (params.command === 'cat' && params.args[0] === '/tmp/dhcp.leases') {
      return {
        exitCode: 0,
        stdout: mockDhcpLeasesOutput,
        stderr: '',
      }
    }

    // Mock Start9 DDNS hostname file
    if (params.command === 'cat' && params.args[0] === '/etc/start9/hostname') {
      return {
        exitCode: 0,
        stdout: 'abc123xyz.start9.net',
        stderr: '',
      }
    }

    // Handle service restart commands
    if (
      params.command === '/etc/init.d/firewall' ||
      params.command === '/etc/init.d/dnsmasq' ||
      params.command === '/etc/init.d/network' ||
      params.command === '/etc/init.d/ddns'
    ) {
      return {
        exitCode: 0,
        stdout: '',
        stderr: '',
      }
    }

    // Handle ubus calls for WAN status
    if (
      params.command === 'ubus' &&
      params.args[0] === 'call' &&
      params.args[1] === 'network.interface.wan'
    ) {
      return {
        exitCode: 0,
        stdout: JSON.stringify({
          'ipv4-address': this.mockWanIpv4.assigned_ip
            ? [{ address: this.mockWanIpv4.assigned_ip, mask: 24 }]
            : [],
          'ipv6-address': this.mockWanIpv6.assigned_ipv6
            ? [{ address: this.mockWanIpv6.assigned_ipv6, mask: 64 }]
            : [],
        }),
        stderr: '',
      }
    }

    return {
      exitCode: 0,
      stdout: 'success',
      stderr: '',
    }
  }

  async getFile(params: GetFileReq): Promise<GetFileRes> {
    await pauseFor(250)
    return {
      modified: new Date().toISOString(),
      contents: '',
    }
  }

  async setFile(params: SetFileReq): Promise<null> {
    await pauseFor(250)
    return null
  }

  async getUci<T extends Record<string, UciFile<any>>>(
    params: GetUciReq,
  ): Promise<T> {
    await pauseFor(250)

    return params.names.reduce(
      (obj, name) => ({
        ...obj,
        [name]: mockUci[name],
      }),
      {} as T,
    )
  }

  async setUci<T extends string[]>(params: SetUciReq): Promise<SetUciRes<T>> {
    await pauseFor(250)

    const isoString = new Date().toISOString()

    // Actually update the mock data
    for (const name of Object.keys(params)) {
      if (mockUci[name]) {
        mockUci[name] = {
          ...params[name],
          modified: isoString,
        }
      }
    }

    return Object.keys(params).reduce(
      (obj, name) => ({
        ...obj,
        [name]: isoString,
      }),
      {} as SetUciRes<T>,
    )
  }

  private mockSystemInfo: SystemInfoRes = {
    version: '1.0.0',
    language: 'en_US',
    date: new Date().toISOString(),
    theme: 'system',
    remoteAccess: 'default',
    timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
  }

  async systemInfo(_timeout?: number): Promise<SystemInfoRes> {
    await pauseFor(250)
    return { ...this.mockSystemInfo, date: new Date().toISOString() }
  }

  async systemNewerVersions(): Promise<VersionInfo[]> {
    await pauseFor(250)

    return [
      {
        version: '1.0.1',
        releaseNotes: `## Bug Fixes

- Resolved issue with DHCP lease renewals
- Fixed port forwarding rules not persisting after reboot
- Corrected timezone display in system logs

## Security Updates

- Updated OpenSSL to latest version
- Patched CVE-2024-1234 vulnerability`,
      },
      {
        version: '1.0.2',
        releaseNotes: `## What's New

- Improved Wi-Fi stability and range
- Fixed intermittent connection drops on 5GHz band
- Added support for WPA3 security protocol
- Performance improvements for VPN connections`,
      },
    ]
  }

  async systemUpdate(params: SystemUpdateReq): Promise<SystemUpdateRes> {
    await pauseFor(500)
    return { target: '1.0.2', progress: 'mock-guid-1234' }
  }

  async systemRestart(): Promise<null> {
    await pauseFor(250)
    return null
  }

  async setPassword(params: SetPasswordReq): Promise<null> {
    await pauseFor(250)
    // Mock validation - in real implementation, backend validates old password
    if (params.oldPassword === '') {
      throw new Error('Invalid old password')
    }
    return null
  }

  async setPreferences(params: SetPreferencesReq): Promise<null> {
    await pauseFor(250)
    if (params.language !== undefined)
      this.mockSystemInfo.language = params.language
    if (params.theme !== undefined) this.mockSystemInfo.theme = params.theme
    if (params.remoteAccess !== undefined) {
      this.mockSystemInfo.remoteAccess = params.remoteAccess
      this.logActivity(
        'system',
        'remote-access',
        `Updated remote access to '${params.remoteAccess}'`,
      )
    }
    return null
  }

  private mockVpnServers: VpnServer[] = this.loadVpnServers()
  private mockNextIpCounter = this.loadIpCounter()

  private loadVpnServers(): VpnServer[] {
    const stored = localStorage.getItem('mock:vpn-servers')
    if (stored) {
      try {
        return JSON.parse(stored)
      } catch {}
    }
    return [
      {
        profile: 'guest',
        label: 'Family',
        enabled: true,
        listen_port: 51820,
        endpoint: '100.65.227.234',
        public_key: 'aB3dE5fG7hI9jK1lM3nO5pQ7rS9tU1vW3xY5zA7bC9d=',
        server_address: '192.168.1.1',
        peers: [],
      },
      {
        profile: 'lan',
        label: 'Matt',
        enabled: false,
        listen_port: 51821,
        endpoint: 'agf5d.start9.me',
        public_key: 'xY5zA7bC9daB3dE5fG7hI9jK1lM3nO5pQ7rS9tU1vW3=',
        server_address: '192.168.1.1',
        peers: [
          {
            name: 'tablet',
            ip: '192.168.1.2',
            public_key: '2JIBoK+Bxe7MJzX9zV+lFjqHxLTvehLp3piEROaNJjw=',
          },
          {
            name: 'Smartphone',
            ip: '192.168.1.3',
            public_key: 'bZIOgRYRGTX9x0OQsN1K+R63EhT2Pgo0WzYatTmzdDU=',
          },
        ],
      },
    ]
  }

  private loadIpCounter(): number {
    const stored = localStorage.getItem('mock:vpn-ip-counter')
    return stored ? parseInt(stored, 10) : 4
  }

  private persistVpnServers(): void {
    localStorage.setItem(
      'mock:vpn-servers',
      JSON.stringify(this.mockVpnServers),
    )
  }

  private persistIpCounter(): void {
    localStorage.setItem('mock:vpn-ip-counter', String(this.mockNextIpCounter))
  }

  async vpnServerList(): Promise<VpnServers> {
    await pauseFor(250)
    return { servers: this.mockVpnServers }
  }

  async vpnServerSet(params: VpnServerSetArgs): Promise<null> {
    await pauseFor(250)
    const existingIndex = this.mockVpnServers.findIndex(
      s => s.profile === params.profile,
    )
    if (existingIndex >= 0) {
      this.mockVpnServers = this.mockVpnServers.map((s, i) =>
        i === existingIndex
          ? { ...s, ...params.config, public_key: s.public_key }
          : s,
      )
    } else {
      const profile = this.mockProfiles.find(
        p => p.interface === params.profile,
      )
      this.mockVpnServers = [
        ...this.mockVpnServers,
        {
          profile: params.profile,
          ...params.config,
          public_key: 'mock' + btoa(String(Date.now())).slice(0, 40) + '=',
          server_address: profile?.gateway_ip ?? '192.168.1.1',
          peers: [],
        },
      ]
    }
    this.persistVpnServers()
    this.logActivity(
      'vpn-server',
      existingIndex >= 0 ? 'updated' : 'created',
      `${existingIndex >= 0 ? 'Updated' : 'Created'} inbound VPN '${params.config.label}'`,
    )
    return null
  }

  async vpnServerDelete(params: VpnServerDeleteArgs): Promise<null> {
    await pauseFor(250)
    const deleted = this.mockVpnServers.find(s => s.profile === params.profile)
    this.mockVpnServers = this.mockVpnServers.filter(
      s => s.profile !== params.profile,
    )
    this.persistVpnServers()
    if (deleted)
      this.logActivity(
        'vpn-server',
        'deleted',
        `Deleted inbound VPN '${deleted.label}'`,
      )
    return null
  }

  async vpnServerPeerAdd(
    params: VpnServerPeerAddArgs,
  ): Promise<VpnServerPeerAddResponse> {
    await pauseFor(250)
    const server = this.mockVpnServers.find(s => s.profile === params.profile)
    const generatedKey =
      params.peer.public_key || btoa(String(Date.now())).slice(0, 40) + '='
    const ip =
      params.peer.ip ||
      `${server?.server_address.replace(/\.\d+$/, '')}.${this.mockNextIpCounter++}`
    this.persistIpCounter()
    const peer = { ...params.peer, public_key: generatedKey, ip }
    this.mockVpnServers = this.mockVpnServers.map(s =>
      s.profile === params.profile ? { ...s, peers: [...s.peers, peer] } : s,
    )
    this.persistVpnServers()
    this.logActivity(
      'vpn-server',
      'updated',
      `Added peer '${params.peer.name}' to inbound VPN`,
    )
    return {
      public_key: generatedKey,
      ip,
      client_config: params.peer.public_key
        ? undefined
        : `[Interface]\nPrivateKey = MOCK_PRIVATE_KEY\nAddress = ${ip}/32\nDNS = ${server?.server_address.replace(/\.\d+$/, '.1') ?? '192.168.1.1'}\n\n[Peer]\nPublicKey = ${server?.public_key ?? 'mockServerKey'}\nEndpoint = ${server?.endpoint ?? 'mock'}:${server?.listen_port ?? 51820}\nAllowedIPs = ${params.peer.route_all ? '0.0.0.0/0, ::/0' : (server?.server_address.replace(/\.\d+$/, '.0/24') ?? '192.168.1.0/24')}`,
    }
  }

  async vpnServerPeerDelete(params: VpnServerPeerDeleteArgs): Promise<null> {
    await pauseFor(250)
    const server = this.mockVpnServers.find(s => s.profile === params.profile)
    const peer = server?.peers.find(p => p.public_key === params.public_key)
    this.mockVpnServers = this.mockVpnServers.map(s =>
      s.profile === params.profile
        ? {
            ...s,
            peers: s.peers.filter(p => p.public_key !== params.public_key),
          }
        : s,
    )
    this.persistVpnServers()
    if (peer)
      this.logActivity(
        'vpn-server',
        'updated',
        `Removed peer '${peer.name}' from inbound VPN`,
      )
    return null
  }

  private mockWifi: WifiConfig = {
    ssid: 'StartOS',
    broadcastSeparately: false,
    radios: {
      default_radio0: {
        band: '2g',
        channel: 'auto',
        enabled: true,
        broadcast: true,
      },
      default_radio1: {
        band: '5g',
        channel: 'auto',
        enabled: true,
        broadcast: true,
      },
    },
    passwords: [
      {
        label: 'Home Network',
        profile: null,
        password: 'password123',
      },
      {
        label: 'Guest Wi-Fi',
        profile: {
          fullname: 'Guest',
          interface: 'guest',
          vlan_tag: 100,
        },
        password: 'guestpass456',
      },
    ],
  }

  async wifiGet(): Promise<WifiConfig> {
    await pauseFor(250)
    return structuredClone(this.mockWifi)
  }

  async wifiSet(params: WifiConfig): Promise<WifiSetResult> {
    await pauseFor(250)
    this.mockWifi = structuredClone(params)
    this.logActivity(
      'wifi',
      'updated',
      `Updated WiFi settings (SSID: '${params.ssid}')`,
    )
    return { pendingPublishedPortDeletions: [] }
  }

  private mockBlackoutWindows: ScheduleWindow[] = [
    {
      startTime: '6:00',
      endTime: '10:00',
      days: [true, false, false, false, false, true, true],
    },
    {
      // Crosses midnight (end < start): renders as a head + tail per night.
      startTime: '22:00',
      endTime: '06:00',
      days: [false, true, true, true, true, true, false],
    },
  ]

  async wifiGeneratePassword(): Promise<string> {
    await pauseFor(100)
    const chars = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789'
    const bytes = crypto.getRandomValues(new Uint8Array(16))
    return Array.from(bytes, b => chars[b % chars.length]).join('')
  }

  async wifiBlackoutGet(): Promise<ScheduleWindow[]> {
    await pauseFor(250)
    return structuredClone(this.mockBlackoutWindows)
  }

  async wifiBlackoutSet(params: ScheduleWindow[]): Promise<null> {
    await pauseFor(250)
    this.mockBlackoutWindows = structuredClone(params)
    this.logActivity(
      'wifi',
      'updated',
      `Updated WiFi blackout schedule (${params.length} window${params.length !== 1 ? 's' : ''})`,
    )
    return null
  }

  private mockProfiles: SecurityProfile[] = [
    {
      fullname: 'Admin',
      interface: 'lan',
      vlan_tag: 1,
      gateway_ip: '192.168.1.1',
      outbound: 'wan',
      lan_access: 'ALL',
      wan_access: 'ALL',
      access_to_new_profiles: true,
      owns_lan: true,
      dns_source: 'system',
    },
    {
      fullname: 'Guest',
      interface: 'guest',
      vlan_tag: 100,
      gateway_ip: '192.168.2.1',
      outbound: 'wan',
      lan_access: 'SAME_PROFILE',
      wan_access: { whitelist: ['1.1.1.1', '8.8.8.8', '9.9.9.9'] },
      access_to_new_profiles: false,
      owns_lan: false,
      dns_override: [
        { address: '1.1.1.1', ssl: true },
        { address: '8.8.8.8', ssl: false },
      ],
      dns_source: 'custom',
    },
    {
      fullname: 'IoT',
      interface: 'iot',
      vlan_tag: 101,
      gateway_ip: '192.168.3.1',
      outbound: 'wan',
      lan_access: 'SAME_PROFILE',
      wan_access: { blacklist: ['192.0.2.0/24', '198.51.100.0/24'] },
      access_to_new_profiles: false,
      owns_lan: false,
      dns_source: 'system',
    },
  ]

  async profilesList(): Promise<ProfileId[]> {
    await pauseFor(250)
    return this.mockProfiles.map(p => ({
      fullname: p.fullname,
      interface: p.interface,
      vlan_tag: p.vlan_tag,
    }))
  }

  async profileGet(params: ProfileIdOpt): Promise<SecurityProfile> {
    await pauseFor(250)
    const profile = this.mockProfiles.find(
      p =>
        (!params.fullname || p.fullname === params.fullname) &&
        (!params.interface || p.interface === params.interface) &&
        (params.vlan_tag === undefined || p.vlan_tag === params.vlan_tag),
    )
    if (!profile) {
      throw new Error('Profile not found')
    }
    return structuredClone(profile)
  }

  async profileCreate(params: ProfileCreateInput): Promise<ProfileId> {
    await pauseFor(250)

    // Allocate a random opaque interface id, mirroring the backend's
    // collision-safe allocation (backend/ctrl/src/profiles.rs
    // `allocate_interface_name`). The id is never shown in the UI (profiles
    // display `fullname`), so it's always random rather than derived from the
    // name — that's what lets a freed-then-reused name get a fresh id instead of
    // colliding. Retry against ids already taken to avoid a chance collision.
    const taken = new Set(this.mockProfiles.map(p => p.interface))
    const randomInterface = () =>
      Array.from({ length: 5 }, () =>
        String.fromCharCode(97 + Math.floor(Math.random() * 26)),
      ).join('')
    let interface_name = randomInterface()
    while (taken.has(interface_name)) {
      interface_name = randomInterface()
    }

    // Auto-assign vlan_tag if not provided
    const existing_tags = this.mockProfiles.map(p => p.vlan_tag)
    const vlan_tag =
      params.vlan_tag ||
      Array.from({ length: 3995 }, (_, i) => i + 101).find(
        t => !existing_tags.includes(t),
      ) ||
      101

    const newProfile: SecurityProfile = {
      fullname: params.fullname || 'Untitled',
      interface: interface_name,
      vlan_tag,
      gateway_ip: params.gateway_ip,
      outbound: params.outbound,
      lan_access: params.lan_access as any, // Cast needed for ProfileIdOpt -> ProfileId
      wan_access: params.wan_access,
      access_to_new_profiles: params.access_to_new_profiles,
      owns_lan: params.owns_lan,
      dns_override: params.dns_override,
      dns_source: params.dns_override?.length
        ? 'custom'
        : params.outbound !== 'wan'
          ? 'vpn'
          : 'system',
    }

    this.mockProfiles = [...this.mockProfiles, newProfile]
    this.logActivity(
      'profile',
      'created',
      `Created profile '${newProfile.fullname}'`,
    )

    return {
      fullname: newProfile.fullname,
      interface: newProfile.interface,
      vlan_tag: newProfile.vlan_tag,
    }
  }

  async profileUpdate(params: ProfileUpdateInput): Promise<ProfileId> {
    await pauseFor(250)

    const old = this.mockProfiles.find(
      p => p.interface === params.interface && p.vlan_tag === params.vlan_tag,
    )
    const newFullname = params.fullname || old?.fullname || ''

    this.mockProfiles = this.mockProfiles.map(p =>
      p.interface === params.interface && p.vlan_tag === params.vlan_tag
        ? {
            ...p,
            fullname: newFullname,
            gateway_ip: params.gateway_ip,
            outbound: params.outbound,
            lan_access: params.lan_access as any,
            wan_access: params.wan_access,
            access_to_new_profiles: params.access_to_new_profiles,
            owns_lan: params.owns_lan,
            dns_override: params.dns_override,
            dns_source: params.dns_override?.length
              ? 'custom'
              : params.outbound !== 'wan'
                ? 'vpn'
                : 'system',
          }
        : p,
    )

    // Cascade fullname change to WiFi passwords and ethernet ports
    if (old && newFullname !== old.fullname) {
      this.mockWifi = {
        ...this.mockWifi,
        passwords: this.mockWifi.passwords.map(pw =>
          pw.profile?.interface === params.interface
            ? { ...pw, profile: { ...pw.profile, fullname: newFullname } }
            : pw,
        ),
      }
      this.mockEthernet = {
        ...this.mockEthernet,
        ports: Object.fromEntries(
          Object.entries(this.mockEthernet.ports).map(([name, port]) => [
            name,
            {
              profile:
                port.profile?.interface === params.interface
                  ? { ...port.profile, fullname: newFullname }
                  : port.profile,
            },
          ]),
        ),
      }
    }

    // Keep LAN IP in sync if this is the LAN-owning profile
    if (params.owns_lan) {
      this.mockLanIpv4 = { ...this.mockLanIpv4, address: params.gateway_ip }
    }

    this.logActivity('profile', 'updated', `Updated profile '${newFullname}'`)

    return {
      fullname: newFullname,
      interface: params.interface,
      vlan_tag: params.vlan_tag,
    }
  }

  async profileDelete(params: ProfileIdOpt): Promise<null> {
    await pauseFor(250)

    const deleted = this.mockProfiles.find(
      p =>
        (!params.fullname || p.fullname === params.fullname) &&
        (!params.interface || p.interface === params.interface) &&
        (params.vlan_tag === undefined || p.vlan_tag === params.vlan_tag),
    )

    this.mockProfiles = this.mockProfiles.filter(p => p !== deleted)

    if (deleted) {
      // Cascade: clear WiFi password profile references
      this.mockWifi = {
        ...this.mockWifi,
        passwords: this.mockWifi.passwords.map(pw =>
          pw.profile?.interface === deleted.interface
            ? { ...pw, profile: null }
            : pw,
        ),
      }

      // Cascade: reset ethernet ports to null
      this.mockEthernet = {
        ...this.mockEthernet,
        ports: Object.fromEntries(
          Object.entries(this.mockEthernet.ports).map(([name, port]) => [
            name,
            {
              profile:
                port.profile?.interface === deleted.interface
                  ? null
                  : port.profile,
            },
          ]),
        ),
      }

      // Cascade: remove VPN servers for this profile
      this.mockVpnServers = this.mockVpnServers.filter(
        s => s.profile !== deleted.interface,
      )
      this.persistVpnServers()

      // Cascade: reset any profiles using this one in lan_access other_profiles
      this.mockProfiles = this.mockProfiles.map(p => {
        if (
          typeof p.lan_access === 'object' &&
          'other_profiles' in p.lan_access
        ) {
          const filtered = p.lan_access.other_profiles.filter(
            op => op.interface !== deleted.interface,
          )
          return {
            ...p,
            lan_access: filtered.length
              ? { other_profiles: filtered }
              : 'SAME_PROFILE',
          }
        }
        return p
      })

      this.logActivity(
        'profile',
        'deleted',
        `Deleted profile '${deleted.fullname}'`,
      )
    }
    return null
  }

  private mockSchedules: Record<string, ScheduleWindow[]> = {}

  async profileScheduleGet(params: {
    interface: string
  }): Promise<ScheduleWindow[]> {
    await pauseFor(100)
    return this.mockSchedules[params.interface] || []
  }

  async profileScheduleSet(params: {
    interface: string
    windows: ScheduleWindow[]
  }): Promise<null> {
    await pauseFor(250)
    this.mockSchedules[params.interface] = params.windows
    return null
  }

  private mockInitialized = true

  async checkInitialized(): Promise<CheckInitializedRes> {
    await pauseFor(250)
    return { initialized: this.mockInitialized }
  }

  async setInitialPassword(params: SetInitialPasswordReq): Promise<null> {
    await pauseFor(250)
    this.mockInitialized = true
    return null
  }

  async setupStatus(): Promise<SetupStatusRes> {
    await pauseFor(250)
    return { setupMode: false, disk: { emmcFound: true, hasFirmware: true } }
  }

  async systemFactoryReset(): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async systemLogs(): Promise<LogsResponse> {
    await pauseFor(250)
    const messages = [
      'daemon.info dnsmasq[1]: read /etc/hosts - 3 names',
      'kern.info kernel: [12345.678] br-lan: port 1(eth0) entered forwarding state',
      'daemon.notice odhcpd[1]: Got a DHCPv6 request',
      'daemon.info dnsmasq-dhcp[1]: DHCPACK(br-lan) 192.168.1.100 aa:bb:cc:dd:ee:ff laptop',
      'daemon.info hostapd: wlan0: STA aa:bb:cc:dd:ee:ff IEEE 802.11: authenticated',
      'daemon.info dnsmasq[1]: query[A] example.com from 192.168.1.100',
      'daemon.info dnsmasq[1]: forwarded example.com to 1.1.1.1',
      'daemon.info dnsmasq[1]: reply example.com is 93.184.216.34',
      'kern.warn kernel: [12400.123] nf_conntrack: table full, dropping packet',
      'daemon.notice netifd: Interface "wan" is now up',
    ]
    const entries: LogEntry[] = []
    for (let i = 0; i < 50; i++) {
      const hour = String(8 + Math.floor(i / 6)).padStart(2, '0')
      const min = String((i * 10) % 60).padStart(2, '0')
      entries.push({
        timestamp: `Mon Mar  3 ${hour}:${min}:00 2026`,
        message: messages[i % messages.length],
      })
    }
    return { entries }
  }

  // --- Device smart endpoint mocks ---

  private mockDeviceHosts = structuredClone(mockDhcpHosts)
  private mockDeviceDefs = structuredClone(MOCK_DEVICE_DEFS)

  async devicesList(): Promise<DeviceFromApi[]> {
    await pauseFor(250)

    return this.mockDeviceDefs.map(def => {
      const mac = def.mac.toUpperCase()
      const host = this.mockDeviceHosts.find(
        h => h.options.mac?.toUpperCase() === mac,
      )
      const device = this.lookupDeviceByMac(mac)
      const profile = this.mockProfiles.find(
        p => p.interface === def.profileInterface,
      )

      // Mirror the backend's server-side name resolution chain:
      // UCI name → DHCP hostname → device-<mac>.
      const dhcpHostname = def.hostname !== '*' ? def.hostname : null
      const name =
        device.name ||
        dhcpHostname ||
        `device-${mac.replace(/:/g, '').slice(-6).toLowerCase()}`

      return {
        mac,
        name,
        hostname: def.hostname,
        status: def.status,
        connection: def.status === 'online' ? def.connection : null,
        ipv4: device.ipv4,
        ipv6: device.ipv6,
        ipv4_static: !!host?.options.ip,
        ipv6_static: !!host?.options.hostid,
        security_profile: profile?.fullname ?? null,
        speed: def.status === 'online' ? def.speed : null,
        data_usage: def.dataUsage,
      }
    })
  }

  async devicesUpdate(params: DeviceUpdateReq): Promise<null> {
    await pauseFor(250)
    const macUpper = params.mac.toUpperCase()
    const existing = this.mockDeviceHosts.find(
      h => h.options.mac?.toUpperCase() === macUpper,
    )
    if (existing) {
      existing.options.name = params.name
      existing.options.ip = params.ipv4_static ? params.ipv4 : undefined
      existing.options.hostid = params.ipv6_static
        ? `1::${parseInt(params.ipv6.split(':').pop() || '0', 16)}`
        : undefined
    } else {
      this.mockDeviceHosts.push({
        type: 'host',
        name: `host_${params.mac.replace(/:/g, '').toLowerCase()}`,
        options: {
          mac: params.mac,
          name: params.name,
          ip: params.ipv4_static ? params.ipv4 : undefined,
          hostid: params.ipv6_static
            ? `1::${parseInt(params.ipv6.split(':').pop() || '0', 16)}`
            : undefined,
          dns: '1',
        },
        lists: {},
      })
    }
    this.logActivity('device', 'updated', `Updated device '${params.name}'`)
    return null
  }

  async devicesForget(params: { mac: string }): Promise<null> {
    await pauseFor(250)
    const macUpper = params.mac.toUpperCase()
    const deleted = this.mockDeviceHosts.find(
      h => h.options.mac?.toUpperCase() === macUpper,
    )
    this.mockDeviceHosts = this.mockDeviceHosts.filter(
      h => h.options.mac?.toUpperCase() !== macUpper,
    )
    this.mockDeviceDefs = this.mockDeviceDefs.filter(
      d => d.mac.toUpperCase() !== macUpper,
    )
    this.logActivity(
      'device',
      'deleted',
      `Forgot device '${deleted?.options.name || params.mac}'`,
    )
    return null
  }

  async devicesDataUsage(
    params: DeviceDataUsageReq,
  ): Promise<DataUsagePointFromApi[]> {
    await pauseFor(250)
    return generateMockDataUsage(params.mac, params.period)
  }

  private mockLanIpv4: LanIpv4Response = {
    address: '192.168.1.1',
    netmask: '255.255.255.0',
  }

  async lanIpv4Get(): Promise<LanIpv4Response> {
    await pauseFor(250)
    return structuredClone(this.mockLanIpv4)
  }

  async lanIpv4Set(params: LanIpv4SetRequest): Promise<null> {
    await pauseFor(250)
    this.mockLanIpv4 = { ...this.mockLanIpv4, address: params.address }
    // Keep Admin profile gateway in sync with LAN IP
    this.mockProfiles = this.mockProfiles.map(p =>
      p.owns_lan ? { ...p, gateway_ip: params.address } : p,
    )
    this.logActivity(
      'lan',
      'ipv4-updated',
      `Updated LAN IPv4 address to ${params.address}`,
    )
    return null
  }

  private mockLanIpv6: LanIpv6Response = {
    slaac: true,
    dhcpv6: true,
    prefix: 64,
    ip6addr: 'fd00::1',
    wan_prefix: 48,
  }

  async lanIpv6Get(): Promise<LanIpv6Response> {
    await pauseFor(250)
    return structuredClone(this.mockLanIpv6)
  }

  async lanIpv6Set(params: LanIpv6SetRequest): Promise<null> {
    await pauseFor(250)
    this.mockLanIpv6 = {
      ...this.mockLanIpv6,
      slaac: params.slaac,
      dhcpv6: params.dhcpv6,
      prefix: params.prefix,
    }
    this.logActivity(
      'lan',
      'ipv6-updated',
      `Updated LAN IPv6 (SLAAC: ${params.slaac ? 'on' : 'off'}, DHCPv6: ${params.dhcpv6 ? 'on' : 'off'})`,
    )
    return null
  }

  // --- WAN smart endpoint mocks ---

  private mockWanIpv4: WanIpv4Response = {
    mode: 'dhcp',
    assigned_ip: '203.0.113.45',
    address: null,
    netmask: null,
    gateway: null,
    username: null,
    password: null,
    device: 'eth1',
  }

  async wanIpv4Get(): Promise<WanIpv4Response> {
    await pauseFor(250)
    return structuredClone(this.mockWanIpv4)
  }

  async wanIpv4Set(params: WanIpv4SetRequest): Promise<null> {
    await pauseFor(250)
    this.mockWanIpv4 = {
      ...this.mockWanIpv4,
      mode: params.mode,
      assigned_ip:
        params.mode === 'static'
          ? (params.address ?? null)
          : params.mode === 'dhcp'
            ? '203.0.113.45'
            : (params.address ?? null),
      address: params.address ?? null,
      netmask: params.netmask ?? null,
      gateway: params.gateway ?? null,
      username: params.username ?? null,
      password: params.password ?? null,
      device: params.device ?? this.mockWanIpv4.device,
    }
    this.logActivity(
      'wan',
      'ipv4-updated',
      `Updated WAN IPv4 (mode: ${params.mode})`,
    )
    return null
  }

  private mockWanIpv6: WanIpv6Response = {
    mode: 'slaac',
    address: null,
    prefix: null,
    gateway: null,
    ip6prefix: null,
    ip6prefixlen: null,
    ip4prefixlen: null,
    assigned_ipv6: '2001:db8::1',
    border_relay: null,
    lan_prefix: null,
  }

  async wanIpv6Get(): Promise<WanIpv6Response> {
    await pauseFor(250)
    return structuredClone(this.mockWanIpv6)
  }

  async wanIpv6Set(params: WanIpv6SetRequest): Promise<null> {
    await pauseFor(250)
    this.mockWanIpv6 = {
      mode: params.mode,
      address: params.address ?? null,
      prefix: params.prefix ?? null,
      gateway: params.gateway ?? null,
      ip6prefix: params.ip6prefix ?? null,
      ip6prefixlen: params.ip6prefixlen ?? null,
      ip4prefixlen: params.ip4prefixlen ?? null,
      border_relay: params.border_relay ?? null,
      lan_prefix: params.lan_prefix ?? null,
      assigned_ipv6:
        params.mode === 'disabled'
          ? null
          : params.mode === 'static'
            ? (params.address ?? null)
            : (this.mockWanIpv6.assigned_ipv6 ?? '2001:db8::1'),
    }
    this.logActivity(
      'wan',
      'ipv6-updated',
      `Updated WAN IPv6 (mode: ${params.mode})`,
    )
    return null
  }

  private mockWanMac: WanMacResponse = {
    strategy: 'router',
    mac: 'AA:BB:CC:DD:EE:01',
    default_mac: 'AA:BB:CC:DD:EE:01',
  }

  async wanMacGet(): Promise<WanMacResponse> {
    await pauseFor(250)
    return structuredClone(this.mockWanMac)
  }

  async wanMacSet(params: WanMacSetRequest): Promise<null> {
    await pauseFor(250)
    this.mockWanMac = {
      ...this.mockWanMac,
      strategy: params.strategy,
      mac:
        params.strategy === 'custom'
          ? (params.mac ?? this.mockWanMac.mac)
          : this.mockWanMac.default_mac,
    }
    return null
  }

  private mockWanDns: WanDnsResponse = {
    mode: 'isp',
    servers: [],
  }

  async wanDnsGet(): Promise<WanDnsResponse> {
    await pauseFor(250)
    return structuredClone(this.mockWanDns)
  }

  async wanDnsSet(params: WanDnsSetRequest): Promise<null> {
    await pauseFor(250)
    this.mockWanDns = {
      mode: params.mode,
      servers: params.servers ?? [],
    }
    return null
  }

  private mockWanDdns: WanDdnsResponse = {
    enabled: true,
    provider: 'dyndns',
    hostname: 'myhost.dyndns.org',
    username: 'myuser',
    password: 'mypass',
    token: null,
    zone: null,
  }

  async wanDdnsGet(): Promise<WanDdnsResponse> {
    await pauseFor(250)
    return structuredClone(this.mockWanDdns)
  }

  async wanDdnsSet(params: WanDdnsSetRequest): Promise<null> {
    await pauseFor(250)
    // Only store fields relevant to the selected provider (matches real backend)
    const providerFields: Record<string, readonly string[]> = {
      start9: [],
      dyndns: ['username', 'password', 'hostname'],
      noip: ['username', 'password', 'hostname'],
      cloudflare: ['token', 'zone', 'hostname'],
      duckdns: ['token', 'hostname'],
      freedns: ['token', 'hostname'],
    }
    const fields = providerFields[params.provider] ?? []
    this.mockWanDdns = {
      enabled: params.enabled,
      provider: params.provider,
      hostname: fields.includes('hostname') ? (params.hostname ?? null) : null,
      username: fields.includes('username') ? (params.username ?? null) : null,
      password: fields.includes('password') ? (params.password ?? null) : null,
      token: fields.includes('token') ? (params.token ?? null) : null,
      zone: fields.includes('zone') ? (params.zone ?? null) : null,
    }
    this.logActivity(
      'wan',
      'ddns-updated',
      `Updated DDNS (provider: ${params.provider}, ${params.enabled ? 'enabled' : 'disabled'})`,
    )
    return null
  }

  // --- Published Ports smart endpoint mocks ---

  private mockPublishedPorts: PublishedPortFromApi[] = [
    {
      id: 'home_assistant',
      enabled: true,
      label: 'Home Assistant',
      device_mac: '00:1A:2B:3C:4D:5E',
      ports: '8123',
      protocol: 'tcp',
      ipv4: true,
      ipv6: true,
      ipv4_public_port: null,
      source: 'any',
      status: 'active',
      status_reason: null,
      device_name: 'Home Server',
      device_ipv4: '192.168.1.100',
      device_ipv6: '2001:db8:abcd:1::100',
    },
    {
      id: 'minecraft',
      enabled: true,
      label: 'Minecraft Server',
      device_mac: '00:1A:2B:3C:4D:5F',
      ports: '25565',
      protocol: 'tcp+udp',
      ipv4: true,
      ipv6: false,
      ipv4_public_port: null,
      source: 'any',
      status: 'active',
      status_reason: null,
      device_name: 'Gaming PC',
      device_ipv4: '192.168.1.101',
      device_ipv6: null,
    },
    {
      id: 'ssh_access',
      enabled: false,
      label: 'SSH Access',
      device_mac: 'DE:AD:BE:EF:CA:FF',
      ports: '22',
      protocol: 'tcp',
      ipv4: true,
      ipv6: true,
      ipv4_public_port: '2222',
      source: '203.0.113.0/24',
      status: 'disabled',
      status_reason: null,
      device_name: null,
      device_ipv4: '192.168.1.103',
      device_ipv6: '2001:db8:abcd:1::103',
    },
  ]

  async publishedPortsList(): Promise<PublishedPortFromApi[]> {
    await pauseFor(250)
    // Re-enrich device info on every fetch to reflect device changes
    return this.mockPublishedPorts.map(p => {
      const device = this.lookupDeviceByMac(p.device_mac)
      return {
        ...p,
        device_name: device.name,
        device_ipv4: device.ipv4,
        device_ipv6: device.ipv6,
      }
    })
  }

  private lookupDeviceByMac(mac: string): {
    name: string | null
    ipv4: string | null
    ipv6: string | null
  } {
    const macUpper = mac.toUpperCase()
    const host = this.mockDeviceHosts.find(
      h => h.options.mac?.toUpperCase() === macUpper,
    )
    const def = this.mockDeviceDefs.find(d => d.mac.toUpperCase() === macUpper)

    // Compute IPv4 from profile gateway + host octet
    let ipv4: string | null = null
    if (host?.options.ip) {
      ipv4 = host.options.ip
    } else if (def) {
      const gateway = this.mockProfiles.find(
        p => p.interface === def.profileInterface,
      )?.gateway_ip
      const subnet = gateway?.split('.').slice(0, 3).join('.') ?? '192.168.1'
      ipv4 = `${subnet}.${def.hostOctet}`
    }

    // Compute IPv6
    let ipv6: string | null = null
    if (def && this.isLanIpv6Enabled()) {
      ipv6 = this.isWanIpv6Enabled()
        ? `2001:db8:abcd:1::${def.hostOctet}`
        : `fd00:1234:5678::${def.hostOctet}`
    }

    return { name: host?.options.name ?? null, ipv4, ipv6 }
  }

  async publishedPortsSet(params: PublishedPortsSetRequest): Promise<null> {
    await pauseFor(250)

    // Auto-reserve static IPs for enabled ports (matches real backend behavior)
    for (const input of params.ports) {
      if (!input.enabled) continue
      const macUpper = input.device_mac.toUpperCase()
      const existing = this.mockDeviceHosts.find(
        h => h.options.mac?.toUpperCase() === macUpper,
      )

      // Reserve IPv4 static if port uses IPv4 and no reservation exists
      if (input.ipv4 && existing && !existing.options.ip) {
        const leaseIp = mockDhcpLeasesOutput
          .split('\n')
          .find(l => l.includes(input.device_mac.toLowerCase()))
          ?.split(/\s+/)[2]
        if (leaseIp) existing.options.ip = leaseIp
      }

      // Reserve IPv6 static if port uses IPv6 and no reservation exists
      if (input.ipv6 && existing && !existing.options.hostid) {
        const def = this.mockDeviceDefs.find(
          d => d.mac.toUpperCase() === macUpper,
        )
        if (def) existing.options.hostid = `1::${def.hostOctet}`
      }
    }

    // Update the mock data — enrich inputs with existing display data
    this.mockPublishedPorts = params.ports.map(input => {
      const existing = this.mockPublishedPorts.find(p => p.id === input.id)
      const device = this.lookupDeviceByMac(input.device_mac)
      return {
        ...input,
        ipv4_public_port: input.ipv4_public_port ?? null,
        status: input.enabled ? ('active' as const) : ('disabled' as const),
        status_reason: input.enabled ? null : null,
        device_name: device.name,
        device_ipv4: device.ipv4,
        device_ipv6: device.ipv6,
      }
    })
    this.logActivity(
      'port-forward',
      'updated',
      `Updated published ports (${params.ports.length} rule${params.ports.length !== 1 ? 's' : ''})`,
    )
    return null
  }

  // --- Outbound VPN (WireGuard Client) smart endpoint mocks ---

  private mockVpnClients: OutboundVpn[] = [
    {
      id: 'wg_proton',
      label: 'Proton',
      target: 'Internet',
      enabled: true,
      used_by: [],
      supports_ipv6: true,
      mtu: null,
    },
    {
      id: 'wg_mullvad',
      label: 'Mullvad',
      target: 'Proton',
      enabled: true,
      used_by: [],
      supports_ipv6: false,
      mtu: 1280,
    },
  ]

  async vpnClientList(): Promise<OutboundVpn[]> {
    await pauseFor(250)
    return structuredClone(
      this.mockVpnClients.map(c => ({
        ...c,
        used_by: this.mockProfiles
          .filter(p => p.outbound === c.id)
          .map(p => p.fullname),
      })),
    )
  }

  async vpnClientCreate(
    params: OutboundVpnCreateRequest,
  ): Promise<OutboundVpnCreateResponse> {
    await pauseFor(250)
    const id = `wg_${params.label.toLowerCase().replace(/[^a-z0-9]/g, '_')}`
    this.mockVpnClients = [
      ...this.mockVpnClients,
      {
        id,
        label: params.label,
        target: params.target,
        enabled: true,
        used_by: [],
        supports_ipv6: /(^|\n)\s*Address\s*=[^\n]*:/.test(params.config),
        // Honor an uncommented MTU line; a commented `#MTU=` is ignored.
        mtu:
          Number(/(^|\n)\s*MTU\s*=\s*(\d+)/i.exec(params.config)?.[2]) || null,
      },
    ]
    this.logActivity(
      'vpn-client',
      'created',
      `Created outbound VPN '${params.label}'`,
    )
    return { id }
  }

  async vpnClientUpdate(params: OutboundVpnUpdateRequest): Promise<null> {
    await pauseFor(250)
    this.mockVpnClients = this.mockVpnClients.map(c =>
      c.id === params.id
        ? { ...c, label: params.label, target: params.target, mtu: params.mtu }
        : c,
    )
    this.logActivity(
      'vpn-client',
      'updated',
      `Updated outbound VPN '${params.label}'`,
    )
    return null
  }

  async vpnClientDelete(params: OutboundVpnDeleteRequest): Promise<null> {
    await pauseFor(250)
    const deleted = this.mockVpnClients.find(c => c.id === params.id)
    this.mockVpnClients = this.mockVpnClients.filter(c => c.id !== params.id)

    if (deleted) {
      // Cascade: reset VPN clients that targeted the deleted VPN
      this.mockVpnClients = this.mockVpnClients.map(c =>
        c.target === deleted.label ? { ...c, target: 'Internet' } : c,
      )
      // Cascade: switch profiles routed through this VPN to WAN
      this.mockProfiles = this.mockProfiles.map(p =>
        p.outbound === deleted.id
          ? { ...p, outbound: 'wan', dns_source: 'system' as const }
          : p,
      )
      this.logActivity(
        'vpn-client',
        'deleted',
        `Deleted outbound VPN '${deleted.label}'`,
      )
    }
    return null
  }

  async vpnClientSetEnabled(
    params: OutboundVpnSetEnabledRequest,
  ): Promise<null> {
    await pauseFor(250)
    const vpn = this.mockVpnClients.find(c => c.id === params.id)
    this.mockVpnClients = this.mockVpnClients.map(c =>
      c.id === params.id ? { ...c, enabled: params.enabled } : c,
    )

    // When disabling, switch all profiles routed through this VPN back to WAN
    if (!params.enabled) {
      this.mockProfiles = this.mockProfiles.map(p =>
        p.outbound === params.id
          ? { ...p, outbound: 'wan', dns_source: 'system' as const }
          : p,
      )
    }

    if (vpn)
      this.logActivity(
        'vpn-client',
        params.enabled ? 'enabled' : 'disabled',
        `${params.enabled ? 'Enabled' : 'Disabled'} outbound VPN '${vpn.label}'`,
      )
    return null
  }

  // --- Ethernet smart endpoint mocks ---

  private mockEthernet: EthernetConfig = {
    wan_ipv6: true,
    wan_port: 'eth0',
    ports: {
      eth0: { profile: null },
      eth1: {
        profile: { fullname: 'Admin', interface: 'lan', vlan_tag: 1 },
      },
      eth2: {
        profile: { fullname: 'Admin', interface: 'lan', vlan_tag: 1 },
      },
      eth3: {
        profile: { fullname: 'Guest', interface: 'guest', vlan_tag: 100 },
      },
    },
  }

  async ethernetGet(): Promise<EthernetConfig> {
    await pauseFor(250)
    return structuredClone(this.mockEthernet)
  }

  async ethernetSet(params: EthernetSetConfig): Promise<EthernetSetResult> {
    await pauseFor(250)
    this.mockEthernet = {
      wan_ipv6: params.wan_ipv6,
      wan_port: params.wan_port,
      ports: Object.fromEntries(
        Object.entries(params.ports).map(([name, port]) => [
          name,
          {
            profile: port.profile
              ? (this.mockProfiles.find(
                  p =>
                    (port.profile!.fullname === undefined ||
                      p.fullname === port.profile!.fullname) &&
                    (port.profile!.interface === undefined ||
                      p.interface === port.profile!.interface) &&
                    (port.profile!.vlan_tag === undefined ||
                      p.vlan_tag === port.profile!.vlan_tag),
                ) ?? null)
              : null,
          },
        ]),
      ),
    }
    const changedPorts = Object.keys(params.ports).join(', ')
    this.logActivity(
      'ethernet',
      'updated',
      `Updated ethernet ports (${changedPorts})`,
    )
    return { pending_published_port_deletions: [] }
  }

  // --- SSH Keys smart endpoint mocks ---

  private mockSshKeys: SshKeyFromApi[] = [
    {
      algorithm: 'ssh-ed25519',
      fingerprint: 'ab:cd:ef:01:23:45:67:89:ab:cd:ef:01:23:45:67:89',
      hostname: 'matt@macbook',
    },
    {
      algorithm: 'ssh-ed25519',
      fingerprint: '12:34:56:78:9a:bc:de:f0:12:34:56:78:9a:bc:de:f0',
      hostname: 'lucy@desktop',
    },
  ]

  async sshKeysList(): Promise<SshKeyFromApi[]> {
    await pauseFor(250)
    return structuredClone(this.mockSshKeys)
  }

  async sshKeysAdd(params: SshKeysAddRequest): Promise<SshKeyFromApi> {
    await pauseFor(250)
    const parts = params.key.trim().split(/\s+/)
    const newKey: SshKeyFromApi = {
      algorithm: parts[0] || 'ssh-ed25519',
      fingerprint: Array.from({ length: 16 }, () =>
        Math.floor(Math.random() * 256)
          .toString(16)
          .padStart(2, '0'),
      ).join(':'),
      hostname: parts.slice(2).join(' ') || '',
    }
    this.mockSshKeys = [...this.mockSshKeys, newKey]
    this.logActivity(
      'ssh',
      'created',
      `Added SSH key '${newKey.hostname || newKey.algorithm}'`,
    )
    return newKey
  }

  async sshKeysDelete(params: SshKeysDeleteRequest): Promise<null> {
    await pauseFor(250)
    const deleted = this.mockSshKeys.find(
      k => k.fingerprint === params.fingerprint,
    )
    this.mockSshKeys = this.mockSshKeys.filter(
      k => k.fingerprint !== params.fingerprint,
    )
    if (deleted)
      this.logActivity(
        'ssh',
        'deleted',
        `Removed SSH key '${deleted.hostname || deleted.algorithm}'`,
      )
    return null
  }

  private mockActivityNextId = 25

  private logActivity(
    category: string,
    action: string,
    summary: string,
    success = true,
    error: string | null = null,
  ): void {
    this.mockActivity.unshift({
      id: this.mockActivityNextId++,
      timestamp: new Date().toISOString(),
      category,
      action,
      success,
      summary,
      error,
    })
  }

  private mockActivity: ActivityEntry[] = Array.from(
    { length: 25 },
    (_, i) => ({
      id: i,
      timestamp: new Date(Date.now() - i * 1000 * 60 * 60 * 24).toISOString(),
      category: ['profile', 'wifi', 'vpn-client', 'device', 'auth'][i % 5],
      action: ['created', 'updated', 'deleted', 'enabled', 'login'][i % 5],
      success: i !== 3,
      summary: [
        "Created profile 'Guest'",
        "Updated WiFi settings (SSID: 'MyNetwork')",
        "Deleted outbound VPN 'Mullvad US'",
        "Failed to enable outbound VPN 'NordVPN'",
        'Logged in',
      ][i % 5],
      error: i === 3 ? 'VpnChainCycle: would create a loop' : null,
    }),
  )

  async activityList(
    params: ActivityListParams = {},
  ): Promise<ActivityListResponse> {
    await pauseFor(100)
    const offset = params.offset ?? 0
    const limit = params.limit ?? 50
    return {
      entries: this.mockActivity.slice(offset, offset + limit),
      total: this.mockActivity.length,
    }
  }

  async activityDelete(params: { id: number }): Promise<null> {
    await pauseFor(100)
    this.mockActivity = this.mockActivity.filter(e => e.id !== params.id)
    return null
  }

  async activityClear(): Promise<null> {
    await pauseFor(100)
    this.mockActivity = []
    return null
  }

  async backupCreate(): Promise<BackupCreateRes> {
    await pauseFor(500)
    return {
      guid: 'mock-backup-guid',
      filename: 'backup-startwrt-2026-03-19.tar.gz',
    }
  }

  async backupRestore(): Promise<BackupRestoreRes> {
    await pauseFor(250)
    return { upload: 'mock-restore-guid' }
  }

  async diagnosticsCreate(): Promise<DiagnosticsCreateRes> {
    await pauseFor(500)
    return {
      guid: 'mock-diag-guid',
      filename: 'diagnostics-startwrt-2026-03-19.log',
    }
  }

  /**
   * Check if WAN IPv6 is enabled in mock UCI data
   */
  private isWanIpv6Enabled(): boolean {
    return this.mockEthernet.wan_ipv6
  }

  /**
   * Check if LAN IPv6 is enabled in mock UCI data
   */
  private isLanIpv6Enabled(): boolean {
    return this.mockLanIpv6.slaac
  }
}

export const mockUci: Record<string, UciFile<UciSection>> = {
  network: {
    sections: [
      {
        type: 'interface',
        name: 'wan',
        options: {
          proto: 'dhcp',
          device: 'eth0.2',
          peerdns: '1',
        },
        lists: { dns: [] },
      },
      {
        type: 'interface',
        name: 'wan6',
        options: {
          proto: 'dhcpv6',
          reqaddress: 'try',
          reqprefix: 'auto',
          device: '@wan',
        },
        lists: {},
      },
      {
        type: 'device',
        name: null,
        options: {
          name: 'eth0.2',
          macaddr: 'AA:BB:CC:DD:EE:01',
        },
        lists: {},
      },
      {
        type: 'interface',
        name: 'lan',
        options: {
          proto: 'static',
          device: 'br-lan',
          ipaddr: '192.168.0.1',
          netmask: '255.255.0.0',
          ip6assign: '64',
          ip6addr: 'fd00::1/64',
        },
        lists: {},
      },
      {
        type: 'device',
        name: 'br_lan',
        options: {
          name: 'br-lan',
          type: 'bridge',
        },
        lists: {
          ports: ['eth1', 'eth2', 'eth3'],
        },
      },
      {
        type: 'device',
        name: 'wan_eth0',
        options: {
          name: 'eth0',
        },
        lists: {},
      },
    ],
    modified: new Date().toISOString(),
  },
  ddns: {
    sections: [
      {
        type: 'service',
        name: 'wan',
        options: {
          enabled: '1',
          service_name: 'dyndns.org',
          username: 'myuser',
          password: 'mypass',
          domain: 'myhost.dyndns.org',
          lookup_host: 'myhost.dyndns.org',
          ip_source: 'network',
          ip_network: 'wan',
        },
        lists: {},
      },
    ],
    modified: new Date().toISOString(),
  },
  dhcp: {
    sections: [dhcpLanSlaacDhcpv6, ...mockDhcpHosts],
    modified: new Date().toISOString(),
  },
  firewall: {
    sections: [],
    modified: new Date().toISOString(),
  },
}
