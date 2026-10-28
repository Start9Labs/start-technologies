import type { z } from 'zod'
import type * as schemas from './types.schemas'
export type AddDnsRecordParams = z.output<typeof schemas.AddDnsRecordParams>
export type AddDnsRecordParamsInput = z.input<typeof schemas.AddDnsRecordParams>
export type AddPinholeParams = z.output<typeof schemas.AddPinholeParams>
export type AddPinholeParamsInput = z.input<typeof schemas.AddPinholeParams>
export type AddPortForwardParams = z.output<typeof schemas.AddPortForwardParams>
export type AddPortForwardParamsInput = z.input<
  typeof schemas.AddPortForwardParams
>
export type AddSubnetParams = z.output<typeof schemas.AddSubnetParams>
export type AddSubnetParamsInput = z.input<typeof schemas.AddSubnetParams>
export type AnyVerifyingKey = z.output<typeof schemas.AnyVerifyingKey>
export type AnyVerifyingKeyInput = z.input<typeof schemas.AnyVerifyingKey>
export type Base64 = z.output<typeof schemas.Base64>
export type Base64Input = z.input<typeof schemas.Base64>
export type CapabilityVerdict = z.output<typeof schemas.CapabilityVerdict>
export type CapabilityVerdictInput = z.input<typeof schemas.CapabilityVerdict>

/**
 * Per-subnet DNS proxy configuration. WireGuard clients on a subnet are pointed
 * at the subnet's in-tunnel server address (`.1`) for DNS; the forward-only proxy
 * listening there resolves every query against the upstream(s) selected here.
 */
export type DnsConfig = z.output<typeof schemas.DnsConfig>

/**
 * Per-subnet DNS proxy configuration. WireGuard clients on a subnet are pointed
 * at the subnet's in-tunnel server address (`.1`) for DNS; the forward-only proxy
 * listening there resolves every query against the upstream(s) selected here.
 */
export type DnsConfigInput = z.input<typeof schemas.DnsConfig>

/**
 * Which upstream a subnet's DNS proxy forwards to. `Device`/`Custom` draw
 * their data from companion fields on [`SetSubnetDnsParams`].
 */
export type DnsMode = z.output<typeof schemas.DnsMode>

/**
 * Which upstream a subnet's DNS proxy forwards to. `Device`/`Custom` draw
 * their data from companion fields on [`SetSubnetDnsParams`].
 */
export type DnsModeInput = z.input<typeof schemas.DnsMode>

/**
 * A DNS record served by the tunnel (injected via RFC 2136 or added manually).
 * `value` is the rdata as text: an IP for A/AAAA, a name for CNAME, etc.
 */
export type DnsRecordEntry = z.output<typeof schemas.DnsRecordEntry>

/**
 * A DNS record served by the tunnel (injected via RFC 2136 or added manually).
 * `value` is the rdata as text: an IP for A/AAAA, a name for CNAME, etc.
 */
export type DnsRecordEntryInput = z.input<typeof schemas.DnsRecordEntry>
export type DnsSource = z.output<typeof schemas.DnsSource>
export type DnsSourceInput = z.input<typeof schemas.DnsSource>
export type GatewayId = z.output<typeof schemas.GatewayId>
export type GatewayIdInput = z.input<typeof schemas.GatewayId>
export type GatewayType = z.output<typeof schemas.GatewayType>
export type GatewayTypeInput = z.input<typeof schemas.GatewayType>
export type HttpRedirectStatus = z.output<typeof schemas.HttpRedirectStatus>
export type HttpRedirectStatusInput = z.input<typeof schemas.HttpRedirectStatus>

/**
 * Per-IPv4 HTTP→HTTPS redirect state. The tunnel runs a redirect on port 80 of
 * every public IPv4 by default; this records the addresses where the user has
 * turned it off (absence = on). The redirect also yields to any port-forward
 * occupying port 80 on that IP, so the two never fight over the port.
 */
export type HttpRedirects = z.output<typeof schemas.HttpRedirects>

/**
 * Per-IPv4 HTTP→HTTPS redirect state. The tunnel runs a redirect on port 80 of
 * every public IPv4 by default; this records the addresses where the user has
 * turned it off (absence = on). The redirect also yields to any port-forward
 * occupying port 80 on that IP, so the two never fight over the port.
 */
export type HttpRedirectsInput = z.input<typeof schemas.HttpRedirects>
export type ListDevicesParams = z.output<typeof schemas.ListDevicesParams>
export type ListDevicesParamsInput = z.input<typeof schemas.ListDevicesParams>
export type NetworkInterfaceType = z.output<typeof schemas.NetworkInterfaceType>
export type NetworkInterfaceTypeInput = z.input<
  typeof schemas.NetworkInterfaceType
>
export type Pem = z.output<typeof schemas.Pem>
export type PemInput = z.input<typeof schemas.Pem>

/**
 * One IPv6 GUA firewall entry, keyed by the exposed `[GUA]:external_port`. The
 * destination is always the same GUA. When `internal_port` is `None` (or equals
 * the key's port) it's a pure firewall pinhole — `ct state new accept`, no NAT.
 * When it differs it's a port-only DNAT to `[GUA]:internal_port` (e.g. 80→443),
 * the v6 analogue of a `PortForward::Dnat`.
 */
export type Pinhole = z.output<typeof schemas.Pinhole>

/**
 * One IPv6 GUA firewall entry, keyed by the exposed `[GUA]:external_port`. The
 * destination is always the same GUA. When `internal_port` is `None` (or equals
 * the key's port) it's a pure firewall pinhole — `ct state new accept`, no NAT.
 * When it differs it's a port-only DNAT to `[GUA]:internal_port` (e.g. 80→443),
 * the v6 analogue of a `PortForward::Dnat`.
 */
export type PinholeInput = z.input<typeof schemas.Pinhole>
export type RemoveDeviceParams = z.output<typeof schemas.RemoveDeviceParams>
export type RemoveDeviceParamsInput = z.input<typeof schemas.RemoveDeviceParams>
export type RemoveDnsRecordParams = z.output<
  typeof schemas.RemoveDnsRecordParams
>
export type RemoveDnsRecordParamsInput = z.input<
  typeof schemas.RemoveDnsRecordParams
>
export type RemovePinholeParams = z.output<typeof schemas.RemovePinholeParams>
export type RemovePinholeParamsInput = z.input<
  typeof schemas.RemovePinholeParams
>
export type RemovePortForwardParams = z.output<
  typeof schemas.RemovePortForwardParams
>
export type RemovePortForwardParamsInput = z.input<
  typeof schemas.RemovePortForwardParams
>
export type Session = z.output<typeof schemas.Session>
export type SessionInput = z.input<typeof schemas.Session>
export type SetAutoPortForwardParams = z.output<
  typeof schemas.SetAutoPortForwardParams
>
export type SetAutoPortForwardParamsInput = z.input<
  typeof schemas.SetAutoPortForwardParams
>
export type SetDeviceWanParams = z.output<typeof schemas.SetDeviceWanParams>
export type SetDeviceWanParamsInput = z.input<typeof schemas.SetDeviceWanParams>
export type SetDnsInjectionParams = z.output<
  typeof schemas.SetDnsInjectionParams
>
export type SetDnsInjectionParamsInput = z.input<
  typeof schemas.SetDnsInjectionParams
>
export type SetHttpRedirectEnabledParams = z.output<
  typeof schemas.SetHttpRedirectEnabledParams
>
export type SetHttpRedirectEnabledParamsInput = z.input<
  typeof schemas.SetHttpRedirectEnabledParams
>
export type SetPasswordParams = z.output<typeof schemas.SetPasswordParams>
export type SetPasswordParamsInput = z.input<typeof schemas.SetPasswordParams>
export type SetPinholeEnabledParams = z.output<
  typeof schemas.SetPinholeEnabledParams
>
export type SetPinholeEnabledParamsInput = z.input<
  typeof schemas.SetPinholeEnabledParams
>
export type SetPortForwardEnabledParams = z.output<
  typeof schemas.SetPortForwardEnabledParams
>
export type SetPortForwardEnabledParamsInput = z.input<
  typeof schemas.SetPortForwardEnabledParams
>
export type SetSubnetIpv6Params = z.output<typeof schemas.SetSubnetIpv6Params>
export type SetSubnetIpv6ParamsInput = z.input<
  typeof schemas.SetSubnetIpv6Params
>
export type SetSubnetWanParams = z.output<typeof schemas.SetSubnetWanParams>
export type SetSubnetWanParamsInput = z.input<typeof schemas.SetSubnetWanParams>
export type ShowConfigParams = z.output<typeof schemas.ShowConfigParams>
export type ShowConfigParamsInput = z.input<typeof schemas.ShowConfigParams>

/**
 * One SNI-demultiplexed hostname route on a shared external port.
 */
export type SniRoute = z.output<typeof schemas.SniRoute>

/**
 * One SNI-demultiplexed hostname route on a shared external port.
 */
export type SniRouteInput = z.input<typeof schemas.SniRoute>
export type SubnetParams = z.output<typeof schemas.SubnetParams>
export type SubnetParamsInput = z.input<typeof schemas.SubnetParams>
export type TunnelCertData = z.output<typeof schemas.TunnelCertData>
export type TunnelCertDataInput = z.input<typeof schemas.TunnelCertData>
export type TunnelUpdateResult = z.output<typeof schemas.TunnelUpdateResult>
export type TunnelUpdateResultInput = z.input<typeof schemas.TunnelUpdateResult>
export type UpdatePinholeLabelParams = z.output<
  typeof schemas.UpdatePinholeLabelParams
>
export type UpdatePinholeLabelParamsInput = z.input<
  typeof schemas.UpdatePinholeLabelParams
>
export type UpdatePortForwardLabelParams = z.output<
  typeof schemas.UpdatePortForwardLabelParams
>
export type UpdatePortForwardLabelParamsInput = z.input<
  typeof schemas.UpdatePortForwardLabelParams
>

/**
 * A WireGuard client's role. A `Server` is a StartOS box that may configure the
 * gateway (DNS injection / auto port-forward); a `Client` is a plain peer with
 * no autoconfig. Stored and sticky — toggling the capability flags never changes
 * it; the migration backfills it from those flags.
 */
export type WgClientKind = z.output<typeof schemas.WgClientKind>

/**
 * A WireGuard client's role. A `Server` is a StartOS box that may configure the
 * gateway (DNS injection / auto port-forward); a `Client` is a plain peer with
 * no autoconfig. Stored and sticky — toggling the capability flags never changes
 * it; the migration backfills it from those flags.
 */
export type WgClientKindInput = z.input<typeof schemas.WgClientKind>
export type AddDeviceParams = z.output<typeof schemas.AddDeviceParams>
export type AddDeviceParamsInput = z.input<typeof schemas.AddDeviceParams>
export type AddKeyParams = z.output<typeof schemas.AddKeyParams>
export type AddKeyParamsInput = z.input<typeof schemas.AddKeyParams>

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export type AuthKeys = z.output<typeof schemas.AuthKeys>

/**
 * Enrolled sign-ins indexed by canonical DER64 SPKI.
 */
export type AuthKeysInput = z.input<typeof schemas.AuthKeys>
export type DnsRecords = z.output<typeof schemas.DnsRecords>
export type DnsRecordsInput = z.input<typeof schemas.DnsRecords>

/**
 * Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted
 * override; `servers` and `source` are derived from it and the link configuration.
 * Without an override or gateway-provided resolvers, the public fallback is used.
 */
export type DnsSettings = z.output<typeof schemas.DnsSettings>

/**
 * Per-gateway DNS intent and effective upstreams. `static_servers` is the persisted
 * override; `servers` and `source` are derived from it and the link configuration.
 * Without an override or gateway-provided resolvers, the public fallback is used.
 */
export type DnsSettingsInput = z.input<typeof schemas.DnsSettings>

/**
 * Whether the gateway reachable via this interface speaks each port-mapping
 * protocol, as last probed. Fed by the watcher's periodic probes and by
 * failure/success evidence from the port-map client, and synced to the db so a
 * chronically uncooperative gateway is visible (and skipped) instead of being
 * retried forever.
 */
export type GatewayPortMapCapabilities = z.output<
  typeof schemas.GatewayPortMapCapabilities
>

/**
 * Whether the gateway reachable via this interface speaks each port-mapping
 * protocol, as last probed. Fed by the watcher's periodic probes and by
 * failure/success evidence from the port-map client, and synced to the db so a
 * chronically uncooperative gateway is visible (and skipped) instead of being
 * retried forever.
 */
export type GatewayPortMapCapabilitiesInput = z.input<
  typeof schemas.GatewayPortMapCapabilities
>
export type IpInfo = z.output<typeof schemas.IpInfo>
export type IpInfoInput = z.input<typeof schemas.IpInfo>
export type Pinholes6 = z.output<typeof schemas.Pinholes6>
export type Pinholes6Input = z.input<typeof schemas.Pinholes6>

/**
 * One external-port forward: an nftables DNAT or an SNI-demultiplexed shared
 * port. Mutually exclusive for a given external address.
 */
export type PortForward = z.output<typeof schemas.PortForward>

/**
 * One external-port forward: an nftables DNAT or an SNI-demultiplexed shared
 * port. Mutually exclusive for a given external address.
 */
export type PortForwardInput = z.input<typeof schemas.PortForward>
export type RemoveKeyParams = z.output<typeof schemas.RemoveKeyParams>
export type RemoveKeyParamsInput = z.input<typeof schemas.RemoveKeyParams>
export type SetDeviceKindParams = z.output<typeof schemas.SetDeviceKindParams>
export type SetDeviceKindParamsInput = z.input<
  typeof schemas.SetDeviceKindParams
>
export type SetSubnetDnsParams = z.output<typeof schemas.SetSubnetDnsParams>
export type SetSubnetDnsParamsInput = z.input<typeof schemas.SetSubnetDnsParams>
export type WebserverInfo = z.output<typeof schemas.WebserverInfo>
export type WebserverInfoInput = z.input<typeof schemas.WebserverInfo>
export type WgConfig = z.output<typeof schemas.WgConfig>
export type WgConfigInput = z.input<typeof schemas.WgConfig>
export type NetworkInterfaceInfo = z.output<typeof schemas.NetworkInterfaceInfo>
export type NetworkInterfaceInfoInput = z.input<
  typeof schemas.NetworkInterfaceInfo
>
export type PortForwards = z.output<typeof schemas.PortForwards>
export type PortForwardsInput = z.input<typeof schemas.PortForwards>
export type WgSubnetClients = z.output<typeof schemas.WgSubnetClients>
export type WgSubnetClientsInput = z.input<typeof schemas.WgSubnetClients>
export type WgSubnetConfig = z.output<typeof schemas.WgSubnetConfig>
export type WgSubnetConfigInput = z.input<typeof schemas.WgSubnetConfig>
export type WgSubnetMap = z.output<typeof schemas.WgSubnetMap>
export type WgSubnetMapInput = z.input<typeof schemas.WgSubnetMap>
export type WgServer = z.output<typeof schemas.WgServer>
export type WgServerInput = z.input<typeof schemas.WgServer>
export type TunnelDatabase = z.output<typeof schemas.TunnelDatabase>
export type TunnelDatabaseInput = z.input<typeof schemas.TunnelDatabase>
