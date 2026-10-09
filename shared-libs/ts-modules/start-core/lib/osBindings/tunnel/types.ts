export type AddDeviceParams = {
  subnet: string
  name: string
  ip: string | null
  /**
   * Client (no autoconfig) or Server (gateway-autoconfig on by default).
   */
  kind: WgClientKind
}
export type AddDeviceParamsInput = {
  subnet: string
  name: string
  ip?: string | null
  /**
   * Client (no autoconfig) or Server (gateway-autoconfig on by default).
   */
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
  /**
   * The client's global IPv6 (GUA) to expose. Must be an address this tunnel
   * delegates to a client — its subnet needs an IPv6 prefix.
   */
  gua: string
  /**
   * External port opened on the GUA.
   */
  externalPort: number
  /**
   * Destination port on the GUA. Omit for a pure pinhole (no NAT, internal ==
   * external); set a different value for a port remap (e.g. 80 -> 443).
   */
  internalPort: number | null
  label: string | null
  /**
   * Number of contiguous ports, counting up from external/internal. Default 1.
   */
  count: number | null
}
export type AddPinholeParamsInput = {
  /**
   * The client's global IPv6 (GUA) to expose. Must be an address this tunnel
   * delegates to a client — its subnet needs an IPv6 prefix.
   */
  gua: string
  /**
   * External port opened on the GUA.
   */
  externalPort: number
  /**
   * Destination port on the GUA. Omit for a pure pinhole (no NAT, internal ==
   * external); set a different value for a port remap (e.g. 80 -> 443).
   */
  internalPort?: number | null
  label?: string | null
  /**
   * Number of contiguous ports, counting up from external/internal. Default 1.
   */
  count?: number | null
}
export type AddPortForwardParams = {
  /**
   * External (WAN) port to forward. The external IP is fixed to the target's
   * WAN so return traffic stays symmetric.
   */
  externalPort: number
  target: string
  label: string | null
  /**
   * Hostnames to SNI-demux on the shared external port. Empty = normal DNAT.
   */
  sni: string[]
  /**
   * Number of contiguous ports to forward (a PCP PORT_SET range), counting up
   * from both `external_port` and the target port. Defaults to 1. Not valid
   * together with SNI demux.
   */
  count: number | null
}
export type AddPortForwardParamsInput = {
  /**
   * External (WAN) port to forward. The external IP is fixed to the target's
   * WAN so return traffic stays symmetric.
   */
  externalPort: number
  target: string
  label?: string | null
  /**
   * Hostnames to SNI-demux on the shared external port. Empty = normal DNAT.
   */
  sni?: string[]
  /**
   * Number of contiguous ports to forward (a PCP PORT_SET range), counting up
   * from both `external_port` and the target port. Defaults to 1. Not valid
   * together with SNI demux.
   */
  count?: number | null
}
export type AddSubnetParams = { name: string }
export type AddSubnetParamsInput = { name: string }
export type AnyVerifyingKey = string
export type AnyVerifyingKeyInput = string

/**
 * The server's enrolled auth keys, keyed by their PEM encoding. Each enrolled
 * key is a sign-in: it carries the same metadata a session used to (when it
 * was created, when it was last used, and the user agent that enrolled it).
 */
export type AuthKeys = { [key: string]: Session }

/**
 * The server's enrolled auth keys, keyed by their PEM encoding. Each enrolled
 * key is a sign-in: it carries the same metadata a session used to (when it
 * was created, when it was last used, and the user agent that enrolled it).
 */
export type AuthKeysInput = { [key: string]: SessionInput }
export type Base64 = string
export type Base64Input = string
export type CapabilityVerdict = {
  /**
   * `None` = never probed.
   */
  supported: boolean | null
  at: string | null
}
export type CapabilityVerdictInput = {
  /**
   * `None` = never probed.
   */
  supported?: boolean | null
  at?: string | null
}

/**
 * Per-subnet DNS proxy configuration. WireGuard clients on a subnet are pointed
 * at the subnet's in-tunnel server address (`.1`) for DNS; the forward-only proxy
 * listening there resolves every query against the upstream(s) selected here.
 */
export type DnsConfig =
  | {
      /**
       * Forward to the VPS's own system resolvers.
       */
      type: 'default'
    }
  | ({
      /**
       * Forward to a device on this subnet (its WireGuard IP) on port 53.
       */
      type: 'device'
    } & { ip: string })
  | ({
      /**
       * Forward to operator-specified upstream servers (1-3, optional `:port`).
       */
      type: 'custom'
    } & { servers: string[] })

/**
 * Per-subnet DNS proxy configuration. WireGuard clients on a subnet are pointed
 * at the subnet's in-tunnel server address (`.1`) for DNS; the forward-only proxy
 * listening there resolves every query against the upstream(s) selected here.
 */
export type DnsConfigInput =
  | {
      /**
       * Forward to the VPS's own system resolvers.
       */
      type: 'default'
    }
  | ({
      /**
       * Forward to a device on this subnet (its WireGuard IP) on port 53.
       */
      type: 'device'
    } & { ip: string })
  | ({
      /**
       * Forward to operator-specified upstream servers (1-3, optional `:port`).
       */
      type: 'custom'
    } & { servers: string[] })

/**
 * Which upstream a subnet's DNS proxy forwards to. `Device`/`Custom` draw
 * their data from companion fields on [`SetSubnetDnsParams`].
 */
export type DnsMode = 'default' | 'device' | 'custom'

/**
 * Which upstream a subnet's DNS proxy forwards to. `Device`/`Custom` draw
 * their data from companion fields on [`SetSubnetDnsParams`].
 */
export type DnsModeInput = 'default' | 'device' | 'custom'

/**
 * A DNS record served by the tunnel (injected via RFC 2136 or added manually).
 * `value` is the rdata as text: an IP for A/AAAA, a name for CNAME, etc.
 */
export type DnsRecordEntry = {
  name: string
  type: string
  value: string
  ttl: number
  /**
   * The device IP that injected this, or `null` for a manual record.
   */
  source: string | null
}

/**
 * A DNS record served by the tunnel (injected via RFC 2136 or added manually).
 * `value` is the rdata as text: an IP for A/AAAA, a name for CNAME, etc.
 */
export type DnsRecordEntryInput = {
  name: string
  type: string
  value: string
  ttl: number
  /**
   * The device IP that injected this, or `null` for a manual record.
   */
  source?: string | null
}
export type DnsRecords = DnsRecordEntry[]
export type DnsRecordsInput = DnsRecordEntryInput[]
export type GatewayId = string
export type GatewayIdInput = string

/**
 * Whether the gateway reachable via this interface speaks each port-mapping
 * protocol, as last probed. Fed by the watcher's periodic probes and by
 * failure/success evidence from the port-map client, and synced to the db so a
 * chronically uncooperative gateway is visible (and skipped) instead of being
 * retried forever.
 */
export type GatewayPortMapCapabilities = {
  pcp: CapabilityVerdict
  natPmp: CapabilityVerdict
  upnp: CapabilityVerdict
  /**
   * The PCP server answers ANNOUNCE with the Start9 capability marker, i.e.
   * it honors OPTION_HOSTNAME (SNI demux).
   */
  pcpHostname: CapabilityVerdict
}

/**
 * Whether the gateway reachable via this interface speaks each port-mapping
 * protocol, as last probed. Fed by the watcher's periodic probes and by
 * failure/success evidence from the port-map client, and synced to the db so a
 * chronically uncooperative gateway is visible (and skipped) instead of being
 * retried forever.
 */
export type GatewayPortMapCapabilitiesInput = {
  pcp: CapabilityVerdictInput
  natPmp: CapabilityVerdictInput
  upnp: CapabilityVerdictInput
  /**
   * The PCP server answers ANNOUNCE with the Start9 capability marker, i.e.
   * it honors OPTION_HOSTNAME (SNI demux).
   */
  pcpHostname: CapabilityVerdictInput
}
export type GatewayType = 'inbound-outbound' | 'outbound-only'
export type GatewayTypeInput = 'inbound-outbound' | 'outbound-only'
export type HttpRedirectStatus = {
  ip: string
  /**
   * Whether the redirect is on for this IP (default true).
   */
  enabled: boolean
  /**
   * Whether a port-forward already occupies port 80 on this IP, in which case
   * the redirect yields and does not bind.
   */
  forwarded: boolean
}
export type HttpRedirectStatusInput = {
  ip: string
  /**
   * Whether the redirect is on for this IP (default true).
   */
  enabled: boolean
  /**
   * Whether a port-forward already occupies port 80 on this IP, in which case
   * the redirect yields and does not bind.
   */
  forwarded: boolean
}

/**
 * Per-IPv4 HTTP→HTTPS redirect state. The tunnel runs a redirect on port 80 of
 * every public IPv4 by default; this records the addresses where the user has
 * turned it off (absence = on). The redirect also yields to any port-forward
 * occupying port 80 on that IP, so the two never fight over the port.
 */
export type HttpRedirects = { disabled: string[] }

/**
 * Per-IPv4 HTTP→HTTPS redirect state. The tunnel runs a redirect on port 80 of
 * every public IPv4 by default; this records the addresses where the user has
 * turned it off (absence = on). The redirect also yields to any port-forward
 * occupying port 80 on that IP, so the two never fight over the port.
 */
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
  /**
   * The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence
   * from the update client (`net::dns_update`). A WireGuard gateway only
   * serves the injected `<hostname>.local` while this is `supported`, so
   * only then is the name listed on it.
   */
  dnsUpdate: CapabilityVerdict
}
export type NetworkInterfaceInfoInput = {
  name?: string | null
  secure?: boolean | null
  ipInfo?: IpInfoInput | null
  type?: GatewayTypeInput | null
  portMap?: GatewayPortMapCapabilitiesInput
  /**
   * The gateway's resolver accepted our last RFC 2136 DNS UPDATE — evidence
   * from the update client (`net::dns_update`). A WireGuard gateway only
   * serves the injected `<hostname>.local` while this is `supported`, so
   * only then is the name listed on it.
   */
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

/**
 * One IPv6 GUA firewall entry, keyed by the exposed `[GUA]:external_port`. The
 * destination is always the same GUA. When `internal_port` is `None` (or equals
 * the key's port) it's a pure firewall pinhole — `ct state new accept`, no NAT.
 * When it differs it's a port-only DNAT to `[GUA]:internal_port` (e.g. 80→443),
 * the v6 analogue of a `PortForward::Dnat`.
 */
export type Pinhole = {
  label: string | null
  enabled: boolean
  /**
   * Contiguous ports opened, counting up from the key's port; `1` for single.
   */
  count: number
  /**
   * Destination port on the GUA; `None` means "same as the external (key) port"
   * — a pure pinhole. A different value makes this a port-DNAT.
   */
  internalPort: number | null
  /**
   * Gateway-created (PCP) vs user-added. Drives the UI Manual/Automatic split.
   */
  auto: boolean
}

/**
 * One IPv6 GUA firewall entry, keyed by the exposed `[GUA]:external_port`. The
 * destination is always the same GUA. When `internal_port` is `None` (or equals
 * the key's port) it's a pure firewall pinhole — `ct state new accept`, no NAT.
 * When it differs it's a port-only DNAT to `[GUA]:internal_port` (e.g. 80→443),
 * the v6 analogue of a `PortForward::Dnat`.
 */
export type PinholeInput = {
  label?: string | null
  enabled?: boolean
  /**
   * Contiguous ports opened, counting up from the key's port; `1` for single.
   */
  count?: number
  /**
   * Destination port on the GUA; `None` means "same as the external (key) port"
   * — a pure pinhole. A different value makes this a port-DNAT.
   */
  internalPort?: number | null
  /**
   * Gateway-created (PCP) vs user-added. Drives the UI Manual/Automatic split.
   */
  auto?: boolean
}
export type Pinholes6 = { [key: string]: Pinhole }
export type Pinholes6Input = { [key: string]: PinholeInput }

/**
 * One external-port forward: an nftables DNAT or an SNI-demultiplexed shared
 * port. Mutually exclusive for a given external address.
 */
export type PortForward =
  | ({ kind: 'dnat' } & {
      target: string
      label: string | null
      enabled: boolean
      /**
       * Contiguous ports forwarded (a PCP PORT_SET range); `1` for single-port.
       */
      count: number
      /**
       * Gateway-created (PCP/UPnP) vs user-added. Drives the UI Manual/Automatic split.
       */
      auto: boolean
    })
  | ({ kind: 'sni' } & {
      /**
       * hostname (lowercase; may be `*.suffix`) -> route.
       */
      routes: { [key: string]: SniRoute }
      /**
       * Hostname-less catch-all for this shared external port. Traffic whose
       * SNI matches no `routes` entry — or that carries no SNI (bare-IP TLS,
       * non-TLS) — is spliced here instead of being dropped. `None` closes the
       * port to unmatched traffic. This lets a bare public IP and named
       * domains share one external port, the bare IP acting as the fallback.
       */
      fallback: SniRoute | null
    })

/**
 * One external-port forward: an nftables DNAT or an SNI-demultiplexed shared
 * port. Mutually exclusive for a given external address.
 */
export type PortForwardInput =
  | ({ kind: 'dnat' } & {
      target: string
      label?: string | null
      enabled?: boolean
      /**
       * Contiguous ports forwarded (a PCP PORT_SET range); `1` for single-port.
       */
      count?: number
      /**
       * Gateway-created (PCP/UPnP) vs user-added. Drives the UI Manual/Automatic split.
       */
      auto?: boolean
    })
  | ({ kind: 'sni' } & {
      /**
       * hostname (lowercase; may be `*.suffix`) -> route.
       */
      routes: { [key: string]: SniRouteInput }
      /**
       * Hostname-less catch-all for this shared external port. Traffic whose
       * SNI matches no `routes` entry — or that carries no SNI (bare-IP TLS,
       * non-TLS) — is spliced here instead of being dropped. `None` closes the
       * port to unmatched traffic. This lets a bare public IP and named
       * domains share one external port, the bare IP acting as the fallback.
       */
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
  /**
   * Remove a single SNI route on `source`; omit to remove the whole forward.
   */
  hostname: string | null
}
export type RemovePortForwardParamsInput = {
  source: string
  /**
   * Remove a single SNI route on `source`; omit to remove the whole forward.
   */
  hostname?: string | null
}
export type Session = {
  /**
   * A friendly name for the key, if one was assigned at enrollment (e.g.
   * tunnel device keys). UI-enrolled keys are unnamed.
   */
  name: string | null
  loggedIn: string
  lastActive: string
  userAgent: string | null
}
export type SessionInput = {
  /**
   * A friendly name for the key, if one was assigned at enrollment (e.g.
   * tunnel device keys). UI-enrolled keys are unnamed.
   */
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
export type SetHttpRedirectEnabledParams = {
  /**
   * The public IPv4 whose default-on port-80 HTTP→HTTPS redirect to toggle.
   */
  ip: string
  enabled: boolean
}
export type SetHttpRedirectEnabledParamsInput = {
  /**
   * The public IPv4 whose default-on port-80 HTTP→HTTPS redirect to toggle.
   */
  ip: string
  enabled: boolean
}
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
  /**
   * Toggle a single SNI route on `source`; omit for a DNAT forward.
   */
  hostname: string | null
}
export type SetPortForwardEnabledParamsInput = {
  source: string
  enabled: boolean
  /**
   * Toggle a single SNI route on `source`; omit for a DNAT forward.
   */
  hostname?: string | null
}
export type SetSubnetDnsParams = {
  mode: DnsMode
  /**
   * The selected device's WireGuard IP; required when `mode` is `device`.
   */
  deviceIp: string | null
  /**
   * Up to 3 upstream servers (bare IP or `ip:port`); used when `mode` is `custom`.
   */
  servers: string[]
}
export type SetSubnetDnsParamsInput = {
  mode: DnsModeInput
  /**
   * The selected device's WireGuard IP; required when `mode` is `device`.
   */
  deviceIp?: string | null
  /**
   * Up to 3 upstream servers (bare IP or `ip:port`); used when `mode` is `custom`.
   */
  servers: string[]
}
export type SetSubnetIpv6Params = {
  /**
   * The routed IPv6 prefix delegated to this subnet (e.g. a /64 from Hetzner,
   * a /56 from Linode). `null` disables IPv6 on the subnet.
   */
  prefix: string | null
}
export type SetSubnetIpv6ParamsInput = {
  /**
   * The routed IPv6 prefix delegated to this subnet (e.g. a /64 from Hetzner,
   * a /56 from Linode). `null` disables IPv6 on the subnet.
   */
  prefix?: string | null
}
export type SetSubnetWanParams = { wanIp: string | null }
export type SetSubnetWanParamsInput = { wanIp?: string | null }
export type ShowConfigParams = { subnet: string; ip: string }
export type ShowConfigParamsInput = { subnet: string; ip: string }

/**
 * One SNI-demultiplexed hostname route on a shared external port.
 */
export type SniRoute = {
  target: string
  label: string | null
  enabled: boolean
  /**
   * Gateway-created vs user-added. Drives the UI Manual/Automatic split.
   */
  auto: boolean
}

/**
 * One SNI-demultiplexed hostname route on a shared external port.
 */
export type SniRouteInput = {
  target: string
  label?: string | null
  enabled?: boolean
  /**
   * Gateway-created vs user-added. Drives the UI Manual/Automatic split.
   */
  auto?: boolean
}
export type SubnetParams = { subnet: string }
export type SubnetParamsInput = { subnet: string }
export type TunnelCertData = { key: string; cert: string }
export type TunnelCertDataInput = { key: string; cert: string }
export type TunnelDatabase = {
  webserver: WebserverInfo
  password: string | null
  /**
   * Same key as the StartOS private db, so a 1.1.x db upgrades by serde
   * default (empty — everyone signs in again) with no migration.
   */
  sessionPubkeys: AuthKeys
  gateways: { [key: string]: NetworkInterfaceInfo }
  wg: WgServer
  portForwards: PortForwards
  /**
   * IPv6 GUA firewall pinholes: inbound to a client's own global address is
   * accepted (no NAT — the GUA is directly routable), keyed by the exposed
   * `[GUA]:port`. The v4 analogue is a `PortForward::Dnat`.
   */
  pinholes6: Pinholes6
  dnsRecords: DnsRecords
  httpRedirects: HttpRedirects
}
export type TunnelDatabaseInput = {
  webserver: WebserverInfoInput
  password?: string | null
  /**
   * Same key as the StartOS private db, so a 1.1.x db upgrades by serde
   * default (empty — everyone signs in again) with no migration.
   */
  sessionPubkeys?: AuthKeysInput
  gateways: { [key: string]: NetworkInterfaceInfoInput }
  wg: WgServerInput
  portForwards: PortForwardsInput
  /**
   * IPv6 GUA firewall pinholes: inbound to a client's own global address is
   * accepted (no NAT — the GUA is directly routable), keyed by the exposed
   * `[GUA]:port`. The v4 analogue is a `PortForward::Dnat`.
   */
  pinholes6?: Pinholes6Input
  dnsRecords?: DnsRecordsInput
  httpRedirects?: HttpRedirectsInput
}
export type TunnelUpdateResult = {
  /**
   * "up-to-date", "update-available", or "updating"
   */
  status: string
  /**
   * Currently installed version
   */
  installed: string
  /**
   * Available candidate version
   */
  candidate: string
}
export type TunnelUpdateResultInput = {
  /**
   * "up-to-date", "update-available", or "updating"
   */
  status: string
  /**
   * Currently installed version
   */
  installed: string
  /**
   * Available candidate version
   */
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
  /**
   * Label a single SNI route on `source`; omit to label the DNAT forward.
   */
  hostname: string | null
}
export type UpdatePortForwardLabelParamsInput = {
  source: string
  label?: string | null
  /**
   * Label a single SNI route on `source`; omit to label the DNAT forward.
   */
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

/**
 * A WireGuard client's role. A `Server` is a StartOS box that may configure the
 * gateway (DNS injection / auto port-forward); a `Client` is a plain peer with
 * no autoconfig. Stored and sticky — toggling the capability flags never changes
 * it; the migration backfills it from those flags.
 */
export type WgClientKind = 'client' | 'server'

/**
 * A WireGuard client's role. A `Server` is a StartOS box that may configure the
 * gateway (DNS injection / auto port-forward); a `Client` is a plain peer with
 * no autoconfig. Stored and sticky — toggling the capability flags never changes
 * it; the migration backfills it from those flags.
 */
export type WgClientKindInput = 'client' | 'server'
export type WgConfig = {
  name: string
  key: string
  psk: string
  /**
   * Client (no autoconfig) vs Server (a StartOS box). Sticky; defaulted by the
   * migration from the capability flags.
   */
  kind: WgClientKind
  /**
   * Whether this device may inject DNS records via RFC 2136. Off by default
   * — only enable for devices you trust (it lets the device add records to
   * the tunnel's DNS for every peer to resolve).
   */
  allowDnsInjection: boolean
  /**
   * Whether this device may auto-create port forwards via PCP/IGD. Off by
   * default — paired with `allow_dns_injection` under one "Gateway
   * Autoconfiguration" toggle, but tracked separately so each capability is
   * gated on its own.
   */
  allowAutoPortForward: boolean
  /**
   * SNAT this device's egress to this WAN IP, overriding the subnet's
   * `wan_ip` / the default masquerade. `None` falls back to the subnet rule.
   */
  wanIp: string | null
}
export type WgConfigInput = {
  name: string
  key: string
  psk: string
  /**
   * Client (no autoconfig) vs Server (a StartOS box). Sticky; defaulted by the
   * migration from the capability flags.
   */
  kind?: WgClientKindInput
  /**
   * Whether this device may inject DNS records via RFC 2136. Off by default
   * — only enable for devices you trust (it lets the device add records to
   * the tunnel's DNS for every peer to resolve).
   */
  allowDnsInjection?: boolean
  /**
   * Whether this device may auto-create port forwards via PCP/IGD. Off by
   * default — paired with `allow_dns_injection` under one "Gateway
   * Autoconfiguration" toggle, but tracked separately so each capability is
   * gated on its own.
   */
  allowAutoPortForward?: boolean
  /**
   * SNAT this device's egress to this WAN IP, overriding the subnet's
   * `wan_ip` / the default masquerade. `None` falls back to the subnet rule.
   */
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
  /**
   * SNAT this subnet's egress to this WAN IP instead of `masquerade`. `None`
   * keeps the default masquerade; a per-device `wan_ip` overrides this.
   */
  wanIp: string | null
  /**
   * Routed IPv6 prefix delegated to this subnet, if any. Each host on the
   * subnet (the server and every client) gets one `/128` out of it with its
   * tunnel IPv4 embedded — see [`crate::tunnel::wg6`].
   */
  ipv6: string | null
}
export type WgSubnetConfigInput = {
  name: string
  clients: WgSubnetClientsInput
  dns?: DnsConfigInput
  /**
   * SNAT this subnet's egress to this WAN IP instead of `masquerade`. `None`
   * keeps the default masquerade; a per-device `wan_ip` overrides this.
   */
  wanIp?: string | null
  /**
   * Routed IPv6 prefix delegated to this subnet, if any. Each host on the
   * subnet (the server and every client) gets one `/128` out of it with its
   * tunnel IPv4 embedded — see [`crate::tunnel::wg6`].
   */
  ipv6?: string | null
}
export type WgSubnetMap = { [key: string]: WgSubnetConfig }
export type WgSubnetMapInput = { [key: string]: WgSubnetConfigInput }
