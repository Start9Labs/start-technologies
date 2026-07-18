# StartWRT API Contract

Complete RPC API contract for the StartWRT backend. All endpoints use **JSON-RPC 2.0** over a single HTTP POST endpoint. Request and response types are defined in Rust.

**Goal:** The frontend should never touch UCI files, run shell commands, or read raw files. Every operation goes through a purpose-built RPC method. The backend handles all UCI manipulation, service restarts, and system queries internally.

---

## Shared Types

```rust
/// Identifies a security profile. Used across WiFi, VPN, Ethernet, and Profiles.
#[derive(Serialize, Deserialize)]
struct ProfileId {
    fullname: String,
    interface: String,
    vlan_tag: u16,
}

/// Partial profile identifier for lookups where not all fields are known.
#[derive(Serialize, Deserialize)]
struct ProfileIdOpt {
    fullname: Option<String>,
    interface: Option<String>,
    vlan_tag: Option<u16>,
}

/// A single DNS server entry with protocol info.
#[derive(Serialize, Deserialize)]
struct DnsServer {
    /// IPv4 address of the DNS server
    address: String,
    /// false = plain UDP (port 53), true = DNS-over-HTTPS via SmartDNS
    ssl: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Protocol {
    Tcp,
    Udp,
    #[serde(rename = "tcp+udp")]
    TcpUdp,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Theme {
    Dark,
    Light,
    System,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RemoteAccess {
    Default,
    Never,
    Always,
}
```

---

## 1. Auth

### `auth.login`

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginParams {
    password: String,
    /// Injected by the auth middleware from the HTTP User-Agent header —
    /// clients don't send it.
    user_agent: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginRes {
    session: String,
}
// Response: LoginRes. The middleware also sets the session cookie on the HTTP
// response (`Set-Cookie: session=…; Path=/; SameSite=Strict; HttpOnly`).
// Rate-limited: 3 attempts per 20 seconds.
```

### `auth.logout`

```rust
// Request: {} (empty object)
// Response: null
```

### `auth.set-password`

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetPasswordParams {
    old_password: String,  // wire: `oldPassword`
    new_password: String,  // wire: `newPassword`
}
// Response: null
```

### `auth.verify-password`

Verifies the admin password without creating a session (used internally by the
CLI before prompting for a new one).

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyPasswordParams {
    password: String,
}
// Response: null (Authorization error on a wrong password)
```

### `auth.check-initialized`

No auth required — the UI calls this before login to decide whether to show
first-time setup.

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckInitializedRes {
    initialized: bool,
}
// Backend: true when root has a password hash in /etc/shadow
```

### `auth.set-initial-password`

First-time setup: no session required (registered with login metadata, so it
shares `auth.login`'s rate limiter). Rejected once a password is already set.

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetInitialPasswordParams {
    /// Minimum 12 characters (same rule as auth.set-password)
    password: String,
}
// Response: LoginRes { session } — the caller is immediately authenticated
// Backend: writes the root hash to /etc/shadow, disables the captive portal,
// creates a login session
```

---

## 2. System

### `system.info`

No auth required.

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInfoResponse {
    version: String,
    language: String,
    date: String,  // ISO 8601
    theme: Theme,
    remote_access: String,  // wire: `remoteAccess`
    timezone: String,  // IANA timezone name (e.g. "America/New_York")
}
```

### `system.newer-versions`

No auth required. Queries the Start9 registry for OS versions newer than the running firmware.
Registry URL defaults to `https://startwrt-registry.start9.com` (no trailing slash) and can be
overridden via UCI: `startwrt.system.registry`.

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionInfo {
    version: String,
    release_notes: String,  // Markdown; wire: `releaseNotes`
}
// Response: Vec<VersionInfo>
```

### `system.update`

Initiate an OTA firmware update. Downloads the asset from the registry,
verifies BLAKE3 hash + Ed25519 signatures, then runs `sysupgrade`.
Returns a GUID for WebSocket progress streaming.

```rust
// Request:
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSystemParams {
    /// Registry URL. Defaults to UCI `startwrt.system.registry`, then
    /// `https://startwrt-registry.start9.com`.
    registry: Option<String>,
    target_version: Option<String>, // wire: `targetVersion` (default: latest)
}

// Response:
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSystemRes {
    target: Option<String>,   // Version being installed
    progress: Option<String>, // GUID for /ws/rpc/{guid}
}
```

#### WebSocket progress: `/ws/rpc/{guid}`

Connect via WebSocket to stream `FullProgress` JSON frames:

```typescript
type FullProgress = {
  overall: Progress
  phases: NamedProgress[]
}
type NamedProgress = { name: string; progress: Progress }
type Progress = null | boolean | { done: number; total: number | null; units: string | null }
// null = NotStarted, true = Complete(success), false = Complete(failure)
// Phases: "Downloading firmware", "Verifying integrity", "Applying update"
```

### `system.restart`

```rust
// Request: {}
// Response: null
// Backend: runs `reboot` internally
```

### `system.factory-reset`

```rust
// Request: {}
// Response: null
// Backend: runs `firstboot -y` (wipes the overlay), then reboots after a short
// delay so the response can reach the client
```

### `system.set-preferences`

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetPreferencesRequest {
    language: Option<String>,
    theme: Option<Theme>,
    remote_access: Option<RemoteAccess>,  // wire: `remoteAccess`
}
// Response: null
```

### `system.apply-remote-access`

Internal endpoint (`no_auth`), **not called from the frontend**. Fired by the
`/etc/hotplug.d/iface/99-startwrt-remote-access` hook (in `backend/hotplug/`)
when a WAN interface comes up.

```rust
// Request: {}
// Response: null
// Backend: re-applies the remote-access preference to the firewall against the
// current WAN IPs, reloads the firewall, and kills WAN-side dropbear (SSH)
// sessions so they don't survive a rule change via conntrack ESTABLISHED state
```

### `system.set-timezone`

No auth required — called during initial setup before login.

```rust
#[derive(Deserialize)]
struct SetTimezoneRequest {
    timezone: String,   // IANA timezone name (e.g. "America/New_York")
}
// Response: null
// Backend: resolves the POSIX TZ string from the device's authoritative LuCI
//          zoneinfo table (`ubus call luci getTimezones`); errors if the name is
//          unknown. Sets UCI system.@system[0].zonename (IANA, verbatim) and
//          system.@system[0].timezone (POSIX), reloads system service (writes
//          /etc/TZ), then restarts crond so wall-clock schedules re-base on the
//          new local time. After this, `date`, `cron`, and all libc time
//          functions use local time. "UTC" is accepted as a special case.
```

### `system.get-timezones`

No auth required — backs the settings timezone dropdown.

```rust
// Request: {}
// Response: Vec<String>   // IANA names the device can resolve, "UTC" first then
//                         // the sorted LuCI zoneinfo table keys. No data is
//                         // maintained in our tree; sourced from ubus luci.getTimezones.
```

### `system.logs`

Non-streaming endpoint for CLI usage. Returns all current log entries.

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    timestamp: String,
    message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogsResponse {
    entries: Vec<LogEntry>,
}
// Response: LogsResponse
// Backend: runs `logread`, parses syslog lines
```

### `/api/logs` (WebSocket)

Live-streaming endpoint for the web UI.

1. Client opens WebSocket to `/api/logs`
2. Server spawns `logread -f` (dumps historical entries then follows new ones)
3. Each line is parsed into `LogEntry` and sent as a JSON text frame
4. Connection closes when either side disconnects; child process is killed on drop
5. **Auth required** — a valid session cookie, or the local auth cookie (`local`, checked against `/run/startwrt/rpc.authcookie`), is validated before the WebSocket upgrade (returns 401 if invalid)
6. Each message is a single `LogEntry` JSON object (not wrapped in `LogsResponse`)
7. Unparseable lines are silently dropped (same as the RPC endpoint)

---

## 3. WAN

### `wan.ipv4-get`

Returns WAN IPv4 configuration and the actual assigned IP.

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum WanIpv4Mode {
    Dhcp,
    Static,
    Pppoe,
}

#[derive(Serialize)]
struct WanIpv4Response {
    mode: WanIpv4Mode,
    /// Actual assigned IP (from ubus/runtime), regardless of mode
    assigned_ip: Option<String>,
    /// Static/PPPoE config fields (populated when relevant)
    address: Option<String>,
    netmask: Option<String>,
    gateway: Option<String>,
    /// PPPoE-specific
    username: Option<String>,
    password: Option<String>,
    device: Option<String>,
}
```

### `wan.ipv4-set`

```rust
#[derive(Deserialize)]
struct WanIpv4SetRequest {
    mode: WanIpv4Mode,
    /// Required when mode = Static
    address: Option<String>,
    netmask: Option<String>,
    gateway: Option<String>,
    /// Required when mode = Pppoe
    username: Option<String>,
    password: Option<String>,
    device: Option<String>,
}
// Response: null
// Backend: updates UCI network.wan, restarts network
```

### `wan.ipv6-get`

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum WanIpv6Mode {
    Disabled,
    Slaac,
    Dhcpv6,
    Static,
    #[serde(rename = "6rd")]
    SixRd,
}

#[derive(Serialize)]
struct WanIpv6Response {
    mode: WanIpv6Mode,
    /// Static mode
    address: Option<String>,
    prefix: Option<String>,    // e.g. "/64"
    gateway: Option<String>,
    /// Static mode: LAN prefix pool for sub-delegation, e.g. "2001:db8::/48"
    lan_prefix: Option<String>,
    /// 6RD mode (RFC 5969 parameters from the ISP)
    ip6prefix: Option<String>,
    ip6prefixlen: Option<String>,
    ip4prefixlen: Option<String>,
    border_relay: Option<String>,
    /// Runtime: assigned WAN IPv6 address. GUA-preferred — when the WAN has a
    /// global address it is reported here (the only scope reachable for inbound
    /// forwarding); falls back to the first address on a ULA-only WAN.
    assigned_ipv6: Option<String>,
}
```

### `wan.ipv6-set`

```rust
#[derive(Deserialize)]
struct WanIpv6SetRequest {
    mode: WanIpv6Mode,
    address: Option<String>,
    prefix: Option<String>,
    gateway: Option<String>,
    /// Static mode: LAN prefix pool, e.g. "2001:db8::/48"
    lan_prefix: Option<String>,
    /// 6RD mode
    ip6prefix: Option<String>,
    ip6prefixlen: Option<String>,
    ip4prefixlen: Option<String>,
    border_relay: Option<String>,
}
// Response: null
// Backend: updates UCI network.wan6, restarts network, then restarts odhcpd
```

### `wan.mac-get`

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum MacStrategy {
    Router,
    Custom,
}

#[derive(Serialize)]
struct WanMacResponse {
    strategy: MacStrategy,
    /// Current effective MAC address
    mac: String,
    /// The router's default MAC (shown as read-only reference)
    default_mac: String,
}
```

### `wan.mac-set`

```rust
#[derive(Deserialize)]
struct WanMacSetRequest {
    strategy: MacStrategy,
    /// Required when strategy = Custom
    mac: Option<String>,
}
// Response: null
// Backend: updates UCI network.wan macaddr, restarts network
```

### `wan.dns-get`

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum DnsMode {
    Isp,
    Custom,
}

#[derive(Serialize)]
struct WanDnsResponse {
    mode: DnsMode,
    /// Populated when mode = Custom
    servers: Vec<DnsServer>,
}
```

### `wan.dns-set`

```rust
#[derive(Deserialize)]
struct WanDnsSetRequest {
    mode: DnsMode,
    /// Required when mode = Custom. Each entry specifies address and protocol.
    servers: Option<Vec<DnsServer>>,
}
// Response: null
// Backend: stores custom servers in the `startwrt` config (`system_dns`
// section), sets `peerdns` on wan/wan6 (0 for Custom, 1 for Isp), rewrites
// per-profile DNS forwarding (dnsmasq + firewall DNAT), regenerates the
// SmartDNS config, then runs the full reload_system() (network + smartdns +
// firewall + dnsmasq)
```

### `wan.ddns-get`

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum DdnsProvider {
    Dyndns,
    Noip,
    Cloudflare,
    Duckdns,
    Freedns,
}

#[derive(Serialize)]
struct WanDdnsResponse {
    enabled: bool,
    provider: DdnsProvider,
    /// The hostname registered with the provider
    hostname: Option<String>,
    /// Provider-specific fields
    username: Option<String>,
    password: Option<String>,
    token: Option<String>,
    /// Cloudflare only: the zone's root domain (null for configs saved
    /// before the zone was stored)
    zone: Option<String>,
}
```

A UCI section with an unknown or legacy `service_name` (e.g. `start9`, whose
service never launched) reads back as the disabled default (`enabled: false`,
`provider: dyndns`, all fields null).

### `wan.ddns-set`

```rust
#[derive(Deserialize)]
struct WanDdnsSetRequest {
    enabled: bool,
    provider: DdnsProvider,
    /// Provider-specific fields (which are required depends on provider)
    hostname: Option<String>,
    username: Option<String>,
    password: Option<String>,
    token: Option<String>,
    zone: Option<String>,
}
// Response: null
// Backend: updates UCI ddns config, restarts/stops ddns service
```

For Cloudflare, `zone` is required and must be the domain registered with
Cloudflare (e.g. `example.com`), and `hostname` must be that zone itself
(apex record) or a name under it; anything else is rejected with
`InvalidRequest`. The backend translates to the shape ddns-scripts'
cloudflare script expects: `username 'Bearer'`, `domain 'host@zone'`
(`'@zone'` for the apex), and `use_api_check '1'` so proxied (orange-cloud)
records compare against the record's real content instead of the proxy IP.
Every provider's section also gets `interface 'wan'`, binding it to hotplug
so updates fire immediately on WAN reconnect.

---

## 4. LAN

### `lan.ipv4-get`

```rust
// Request: {}

#[derive(Serialize)]
struct LanIpv4Response {
    /// Full router IP, e.g. "192.168.0.1"
    address: String,
    /// Always /24 (255.255.255.0), but include for completeness
    netmask: String,
}
```

### `lan.ipv4-set`

```rust
#[derive(Deserialize)]
struct LanIpv4SetRequest {
    address: String,
    /// When true, forcibly delete VPN peers that would break due to the change.
    #[serde(default)]
    force: bool,
}
// Response: null
// Backend: updates UCI network.lan ipaddr (netmask /24), restarts network.
// Validation: address must fall inside an RFC 1918 block at one of the
//   selectable /16 boundaries, else ErrorKind::InvalidRequest:
//     10.0.0.0/8     — second octet 0..=255
//     172.16.0.0/12  — second octet 16..=31
//     192.168.0.0/16 — second octet == 168
//   (3rd/4th octets unconstrained.) The same rule is enforced on the admin
//   profile via profiles.set/create; non-admin profiles must additionally
//   share the admin LAN's first two octets (stay inside its /16).
//   The target /24 must not collide with another profile's subnet, else
//   ErrorKind::SubnetCollision (same guard as profiles.create/profiles.edit;
//   on a block change the check applies post-move, since every profile keeps
//   its 3rd octet). A re-set of the current subnet always passes.
```

### `lan.ipv6-get`

```rust
// Request: {}

#[derive(Serialize)]
struct LanIpv6Response {
    slaac: bool,
    dhcpv6: bool,
    /// Prefix delegation length, e.g. 64
    prefix: u8,
    /// Current IPv6 address (if assigned)
    ip6addr: Option<String>,
    /// WAN prefix length (read-only context for the UI)
    wan_prefix: u8,
}
```

### `lan.ipv6-set`

```rust
#[derive(Deserialize)]
struct LanIpv6SetRequest {
    slaac: bool,
    dhcpv6: bool,
    prefix: u8,
}
// Response: null
// Backend: updates UCI dhcp.lan + network.lan, restarts network + odhcpd.
// Disabling SLAAC while any *enabled* IPv6 published-port rule exists is
// rejected (`PublishedPortsUseIpv6`): it would strand every pp_*_v6 pinhole on
// an address no device will hold. The UI locks the toggle for the same reason;
// this backend check is the authoritative one.
```

---

## 5. Ethernet

### `ethernet.get`

```rust
// Request: {}

#[derive(Serialize, Deserialize)]
struct Port<Id = ProfileId> {
    /// Security profile assigned to this port, if any
    profile: Option<Id>,
}

#[derive(Serialize)]
struct Ethernet {
    /// Whether WAN has a DHCPv6 interface
    wan_ipv6: bool,
    /// Which port is the WAN uplink, if any
    wan_port: Option<String>,
    /// Map of physical port name → port config
    ports: BTreeMap<String, Port>,
}
// Response: Ethernet
```

### `ethernet.set`

```rust
// Request: Ethernet fields are flattened in alongside the confirm flag.
#[derive(Deserialize)]
struct EthernetSetRequest {
    #[serde(flatten)]
    ethernet: Ethernet,            // { wan_ipv6, wan_port, ports: BTreeMap<String, Port<ProfileIdOpt>> }
    /// Authorize deleting the published ports returned by a prior unconfirmed call.
    #[serde(default)]
    confirm_published_port_deletion: bool,
}

// A published port that will be deleted because its device is moving to a
// different profile. Shared (snake_case) with wifi.set's result.
#[derive(Serialize)]
struct AffectedPublishedPort {
    id: String,
    label: String,
    device_mac: String,
    device_name: Option<String>,
}

// Response:
#[derive(Serialize)]
struct EthernetSetResult {
    /// Non-empty (and nothing applied) when a port being reassigned to a
    /// different profile has devices with published ports and the caller hasn't
    /// confirmed. Empty once the change is applied.
    pending_published_port_deletions: Vec<AffectedPublishedPort>,
}
// Backend: detects ports changing profile, finds their devices via bridge FDB,
// and the published ports they'd break. Without confirmation it applies nothing
// and returns them; with confirmation it deletes those published ports (firewall
// rules + stale DHCP reservations) atomically with the bridge-VLAN/WAN update,
// then reloads network + firewall.
```

Note: `ProfileId` and `ProfileIdOpt` are shared types defined at the top of the contract.

### `ethernet.edit`

CLI editor flow — opens the current Ethernet config in `$EDITOR`, then calls
`ethernet.set` with published-port deletion implicitly confirmed. Not called by
the web UI.

```rust
// Request: {}
// Response: null
```

---

## 6. Devices

### `devices.list`

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum DeviceStatus {
    Online,
    Offline,
}

#[derive(Serialize)]
struct Device {
    mac: Option<String>,
    /// Fully-resolved display name: UCI static name → live DHCP hostname →
    /// remembered hostname (name cache) → `device-<mac>` placeholder. Always set.
    name: String,
    /// Raw DHCP lease hostname ("*" when unset); a hint for the rename form.
    hostname: Option<String>,
    status: DeviceStatus,
    /// "Ethernet", "Wi-Fi 2.4GHz", "Wi-Fi 5GHz", etc.
    connection: Option<String>,
    ipv4: Option<String>,
    ipv6: Option<String>,
    ipv4_static: bool,
    security_profile: Option<String>,
    /// Live throughput (MB/s, 1 decimal), computed from conntrack byte deltas
    /// between polls. Only set for online devices with a previous sample.
    speed: Option<SpeedData>,
    /// Total data used in the current nlbwmon period (GB, 1 decimal, rx+tx)
    data_usage: Option<f64>,
}

#[derive(Serialize)]
struct SpeedData {
    up: f64,
    down: f64,
}
// Response: Vec<Device>
// Backend: reads DHCP hosts, firewall rules, ARP table, DHCP leases, and a
// persistent name cache (/etc/startwrt/device_names.json) that remembers
// DHCP-advertised hostnames per MAC. The backend resolves the full name
// fallback chain server-side and returns a single `name`.
```

### `devices.update`

```rust
#[derive(Deserialize)]
struct DeviceUpdateRequest {
    mac: String,
    name: String,
    ipv4_static: bool,
    ipv4: String,
}
// Response: null
// Backend: creates/updates DHCP host section, restarts dnsmasq.
// No IPv6 fields: devices choose their own IPv6 addresses (SLAAC), so there is
// no user-facing IPv6 reservation. The host section's `hostid` is backend
// bookkeeping pinned by published-ports; this endpoint leaves it untouched.
```

### `devices.forget`

```rust
#[derive(Deserialize)]
struct DeviceForgetRequest {
    mac: String,
}
// Response: null
// Backend: removes the DHCP host for the MAC and its remembered name, flushes
// matching neighbor/ARP entries (`ip neigh del`), and rewrites every dnsmasq
// lease file (the base file plus per-profile /tmp/dhcp.leases.dns_*) with
// dnsmasq stopped (stop → edit → start), so the in-memory lease can't
// resurrect the entry. A still-connected device reappears on its next
// network activity.
```

### `devices.data-usage`

```rust
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum DataUsagePeriod {
    Week,
    Month,
    #[serde(rename = "3months")]
    ThreeMonths,
}

#[derive(Deserialize)]
struct DeviceDataUsageRequest {
    mac: String,
    period: DataUsagePeriod,
}

#[derive(Serialize)]
struct DataUsagePoint {
    /// Unix timestamp (seconds) at UTC midnight of the day this point covers
    timestamp: u64,
    /// Bytes uploaded that day
    upload: u64,
    /// Bytes downloaded that day
    download: u64,
}
// Response: Vec<DataUsagePoint>, daily granularity, oldest first.
// Days the device had no traffic (or the archive is missing) are zero-filled.
// Backend: fans out `nlbw -c json -g mac -t YYYY-MM-DD` over the requested window.
```

---

## 7. Published Ports

### `published-ports.list`

Returns all port forwarding rules with enriched device info and status.

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum PublishedPortStatus {
    Active,
    /// One protocol works but the other can't: IPv4 unavailable (e.g. CGNAT), or
    /// the IPv6 forward is stranded on an old prefix ("IPv6 address out of date").
    Partial,
    /// Device offline or identity mismatch
    Paused,
    /// Failed to apply rule, no usable address, or (IPv6-only) the forward is
    /// stranded on an old delegated prefix ("IPv6 address out of date" — the
    /// device's current GUA is in a different /64; normally repaired by the
    /// ipv6_tracker-triggered reconcile, or the wan6 hotplug's).
    Error,
    Disabled,
}

#[derive(Serialize)]
struct PublishedPort {
    id: String,
    enabled: bool,
    label: String,
    device_mac: String,
    /// Internal port or range, e.g. "8123" or "27015-27030"
    ports: String,
    protocol: Protocol,
    ipv4: bool,
    ipv6: bool,
    /// External port for IPv4 (if different from internal)
    ipv4_public_port: Option<String>,
    /// "any" or CIDR like "203.0.113.0/24"
    source: String,
    // --- Enriched by backend ---
    status: PublishedPortStatus,
    status_reason: Option<String>,
    device_name: Option<String>,
    device_ipv4: Option<String>,
    device_ipv6: Option<String>,
}
// Response: Vec<PublishedPort>
// Backend: reads firewall redirects + rules, resolves device info from DHCP/ARP
```

### `published-ports.set`

Replaces the full list of published ports.

```rust
#[derive(Deserialize)]
struct PublishedPortInput {
    id: String,
    enabled: bool,
    label: String,
    device_mac: String,
    ports: String,
    protocol: Protocol,
    ipv4: bool,
    ipv6: bool,
    ipv4_public_port: Option<String>,
    source: String,
}

#[derive(Deserialize)]
struct PublishedPortsSetRequest {
    ports: Vec<PublishedPortInput>,
}
// Response: null
// Backend: rebuilds firewall redirect+rule sections, resolves device IPs, restarts firewall
```

Validation (errors with `MissingDeviceAddress`, rejecting the whole request):

- An enabled `ipv4` port whose device has no resolvable IPv4 address.
- An enabled `ipv6` port when IPv6 publishing is impossible — any of:
  the router has no global IPv6 prefix delegated; the device resolved to a ULA
  address; or the device is online but has only a link-local address. ULA and
  link-local are unreachable from the WAN, so the request is rejected rather than
  silently saving a dead IPv6 forward. Only a _genuinely offline_ device (no IPv6
  seen at all) on a GUA-capable router is not rejected; its IPv6 rule is deferred
  by the rule-creation GUA guard so a briefly-offline device doesn't fail an
  otherwise-valid save.

An `ipv6` port's rule pins the tracker-elected stable GUA the device holds
within a currently-assigned prefix; with none (e.g. saved mid-prefix-rotation),
the resolved address is used as-is and the tracker's reconcile retargets the
rule within seconds of the device acquiring an in-prefix address. There is no
DHCP `hostid` pinning (devices choose their own IPv6 addresses; odhcpd also
misparses colon-separated hostids) — `reconcile` keeps the rule current
instead. Legacy hostids written by older releases are left in place and still
serve as a last-resort reconcile fallback.

### `published-ports.reconcile`

```rust
// Request: {}
// Response: null
```

Internal endpoint (`no_auth`), **not called from the frontend**. Fired two
ways: by the `/etc/hotplug.d/iface/99-startwrt-published-ports` hook on `wan6`
`ifup`/`ifupdate` (i.e. when the ISP-delegated IPv6 prefix changes) — the CLI
forwards the call to the daemon (`with_call_remote`), where the `ipv6_tracker`'s
live history lives — and by the `ipv6_tracker`'s own debounce whenever a
rule-relevant device's live address set or election changes. Recomputes the
`dest_ip` of every `pp_*_v6` forward: the tracker-elected stable GUA the device
currently holds within any currently-assigned LAN-side prefix (admin LAN or
profile bridge /64s), otherwise `bridge_prefix ++ suffix` recombined with the
/64 of the device's own bridge — the suffix from tracker history when the
elected address is EUI-64, else a legacy stored `hostid` (suppressed when
history proves the device does not use EUI-64). A forward with neither source
is left untouched. Reloads the firewall only if something changed. No-ops when
the router currently has no global prefix (a flap to "none" never wipes rules).

---

## 8. Outbound VPN (WireGuard Clients)

### `vpn-client.list`

```rust
// Request: {}

#[derive(Serialize)]
struct OutboundVpn {
    /// WireGuard interface name (e.g. "wg_proton")
    id: String,
    label: String,
    /// "Internet" or another VPN's label (for chaining)
    target: String,
    enabled: bool,
    /// Which security profiles route through this VPN
    used_by: Vec<String>,
    /// True when the WG config supplied at least one IPv6 `Address`. Non-admin
    /// profiles pointed at an outbound VPN with `supports_ipv6 == false` get
    /// IPv6 disabled (RA/DHCPv6) to avoid leaking around the tunnel. The admin
    /// (LAN-owning) profile is exempt: its RA/DHCPv6 are owned solely by the
    /// LAN IPv6 settings, and a v4-only VPN outbound fails IPv6 closed via the
    /// kill-switch route instead (LAN devices keep local ULA IPv6).
    supports_ipv6: bool,
    /// Interface MTU if explicitly set, else null (kernel default ~1420).
    mtu: Option<u16>,
}
// Response: Vec<OutboundVpn>
```

### `vpn-client.create`

```rust
#[derive(Deserialize)]
struct OutboundVpnCreateRequest {
    label: String,
    target: String,
    /// Raw WireGuard .conf file contents
    config: String,
}

#[derive(Serialize)]
struct OutboundVpnCreateResponse {
    /// The assigned interface ID
    id: String,
}
// Backend: parses WireGuard config, creates UCI interface+peer, restarts network.
// An uncommented `MTU` in the .conf (1280–1500) becomes `option mtu`; a
// commented `#MTU` is ignored. The dedicated vpn_<X> egress zone also gets
// `option mtu_fix 1` (TCP MSS clamp) as a backstop against too-high MTU.
//
// Validation (InvalidValue on failure): the config MUST be a WireGuard .conf —
// it requires `[Interface]` + `[Peer]` headers, a valid PrivateKey/PublicKey
// (base64 32-byte), at least one interface `Address`, and a peer `Endpoint`.
// OpenVPN configs (no INI headers) are rejected with an OpenVPN-specific
// message. The web UI mirrors this with an async file-content check before
// submit, but the backend is the authoritative gate.
```

### `vpn-client.update`

```rust
#[derive(Deserialize)]
struct OutboundVpnUpdateRequest {
    id: String,
    label: String,
    target: String,
    /// Desired interface MTU (1280–1500). null/absent clears it (inherit the
    /// kernel default). UCI is the single source of truth — there is no stored
    /// .conf; the web edit form always submits the field's current value.
    #[serde(default)]
    mtu: Option<u16>,
}
// Response: null
// Backend: updates label+target metadata and the interface `mtu` option.
// Bounces the WG interface only when the MTU actually changed.
```

### `vpn-client.delete`

```rust
#[derive(Deserialize)]
struct OutboundVpnDeleteRequest {
    id: String,
}
// Response: null
// Backend: removes UCI interface+peer sections, restarts network
```

### `vpn-client.set-enabled`

```rust
#[derive(Deserialize)]
struct OutboundVpnSetEnabledRequest {
    id: String,
    enabled: bool,
}
// Response: null
// Backend: sets/clears UCI disabled flag, restarts network
```

---

## 9. Inbound VPN (WireGuard Servers)

> These endpoints already exist. Included for completeness.

### `vpn-server.list`

```rust
// Request: {}

#[derive(Serialize)]
struct VpnServer {
    profile: String,
    label: String,
    enabled: bool,
    listen_port: u16,
    endpoint: String,
    public_key: String,
    server_address: String,
    peers: Vec<VpnServerPeer>,
}

#[derive(Serialize)]
struct VpnServerPeer {
    name: String,
    ip: Option<String>,
    public_key: Option<String>,
    preshared_key: Option<String>,
    /// Route all traffic (LAN + WAN) through tunnel. Default/absent = split tunnel (LAN only).
    route_all: Option<bool>,
}

#[derive(Serialize)]
struct VpnServerListResponse {
    servers: Vec<VpnServer>,
}
```

### `vpn-server.set`

```rust
#[derive(Deserialize)]
struct VpnServerSetRequest {
    profile: String,
    config: VpnServerConfig,
}

#[derive(Deserialize)]
struct VpnServerConfig {
    label: String,
    enabled: bool,
    listen_port: u16,
    endpoint: String,
    private_key: Option<String>,
}
// Response: null
```

### `vpn-server.delete`

```rust
#[derive(Deserialize)]
struct VpnServerDeleteRequest {
    profile: String,
}
// Response: null
```

### `vpn-server.peer-add`

```rust
#[derive(Deserialize)]
struct VpnServerPeerAddRequest {
    profile: String,
    peer: VpnServerPeerInput,
}

#[derive(Deserialize)]
struct VpnServerPeerInput {
    name: String,
    ip: Option<String>,
    public_key: Option<String>,
    preshared_key: Option<String>,
    /// Route all traffic (LAN + WAN) through tunnel. Default/absent = split tunnel (LAN only).
    route_all: Option<bool>,
}

#[derive(Serialize)]
struct VpnServerPeerAddResponse {
    /// WireGuard client config (only when server generates the keypair)
    client_config: Option<String>,
    public_key: String,
    ip: String,
}
```

### `vpn-server.peer-delete`

```rust
#[derive(Deserialize)]
struct VpnServerPeerDeleteRequest {
    profile: String,
    public_key: String,
}
// Response: null
```

---

## 10. WiFi

> These endpoints already exist. Included for completeness.

### `wifi.get`

```rust
// Request: {}

#[derive(Serialize)]
struct WifiRadio {
    band: String,       // "2g", "5g"
    channel: String,    // "auto", "1", "36", etc.
    enabled: bool,
    broadcast: bool,
}

#[derive(Serialize)]
struct WifiPassword {
    label: String,
    profile: Option<ProfileId>,
    password: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WifiConfig {
    ssid: String,
    broadcast_separately: bool,
    radios: HashMap<String, WifiRadio>,
    passwords: Vec<WifiPassword>,
}
// Response: WifiConfig
```

### `wifi.set`

```rust
// Request: WifiConfig fields (camelCase) flattened in, plus a confirm flag.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WifiSetRequest {
    #[serde(flatten)]
    wifi: WifiConfig,                       // same structure as wifi.get's response
    /// Authorize deleting the published ports returned by a prior unconfirmed call.
    #[serde(default)]
    confirm_published_port_deletion: bool,  // serialized as `confirmPublishedPortDeletion`
}

// Response:
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WifiSetResult {
    /// Non-empty (and nothing applied) when reassigning a password to a different
    /// profile would break published ports for devices on the vacated profile and
    /// the caller hasn't confirmed. Empty once applied. Entries are the shared
    /// snake_case `AffectedPublishedPort` (see ethernet.set).
    pending_published_port_deletions: Vec<AffectedPublishedPort>,  // `pendingPublishedPortDeletions`
}
// Backend: a profile losing its last WiFi password is "vacated"; its WiFi devices
// (matched by current profile, since per-SSID mapping isn't available) move off
// that subnet. Without confirmation it applies nothing and returns the published
// ports that would break; with confirmation it deletes them (firewall rules +
// stale DHCP reservations) atomically with the WiFi update, then reloads firewall.
```

### `wifi.blackout-get`

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlackoutWindow {
    start_time: String,  // "HH:MM"; wire: `startTime`
    end_time: String,    // "HH:MM"; wire: `endTime`
    /// [Sun, Mon, Tue, Wed, Thu, Fri, Sat]
    days: [bool; 7],
}
// Response: Vec<BlackoutWindow>
```

A window may cross midnight: when `endTime < startTime` (e.g. `22:00`–`06:00`) it runs
from `startTime` on each selected day until `endTime` the _following_ day, and the
backend shifts the closing cron edge forward one day. Equal `startTime`/`endTime`
denotes a full 24-hour window on the selected days and is allowed. On `*-set`, windows
that overlap on the weekly timeline (wrap-aware) are rejected with `InvalidValue`, as is
a schedule that covers the entire week with no gap (the deconflicted crontab projection
would have zero edges — disable WiFi directly instead). The same wrap, overlap, and
full-week semantics apply to the profile `profiles.schedule-get` /
`profiles.schedule-set` windows.

### `wifi.blackout-set`

```rust
#[derive(Deserialize)]
struct BlackoutSetRequest {
    windows: Vec<BlackoutWindow>,
}
// Response: null
```

### `wifi.edit`

CLI editor flow — opens the current WiFi config in `$EDITOR`, then calls
`wifi.set` with published-port deletion implicitly confirmed. Not called by the
web UI.

```rust
// Request: {}
// Response: null
```

### `wifi.generate-password`

```rust
// Request: {}
// Response: String — a random 16-character alphanumeric password
```

---

## 11. Security Profiles

> These endpoints already exist. Included for completeness.

### `profiles.list`

```rust
// Request: {}
// Response: Vec<ProfileId>
```

### `profiles.get`

```rust
// Request: ProfileIdOpt

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum LanAccess {
    /// "ALL" or "SAME_PROFILE"
    Preset(String),
    /// Explicit list of allowed profiles
    Whitelist { other_profiles: Vec<ProfileIdOpt> },
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum WanAccess {
    /// "ALL" or "NONE"
    Preset(String),
    Whitelist { whitelist: Vec<String> },
    Blacklist { blacklist: Vec<String> },
}

#[derive(Serialize)]
struct SecurityProfile {
    fullname: String,
    interface: String,
    vlan_tag: u16,
    gateway_ip: String,
    /// "wan" for default WAN, or outbound VPN interface name
    outbound: String,
    lan_access: LanAccess,
    wan_access: WanAccess,
    access_to_new_profiles: bool,
    owns_lan: bool,
    /// Always present in responses — an empty array when unset, never null
    #[serde(default)]
    dns_override: Vec<DnsServer>,
    /// "system", "custom", or "vpn"
    dns_source: String,
}
// Response: SecurityProfile
```

### `profiles.create`

```rust
#[derive(Deserialize)]
struct ProfileCreateRequest {
    fullname: Option<String>,
    interface: Option<String>,
    vlan_tag: Option<u16>,
    gateway_ip: String,
    outbound: String,
    lan_access: LanAccess,
    wan_access: WanAccess,
    access_to_new_profiles: bool,
    owns_lan: bool,
    #[serde(default)]
    dns_override: Vec<DnsServer>,
    /// "system", "custom", or "vpn"
    #[serde(default)]
    dns_source: String,
}
// Response: ProfileId
// Validation: gateway_ip must stay inside the LAN network block (see
//   lan.ipv4-set). owns_lan profiles must be a valid RFC 1918 selection;
//   others must share the admin LAN's first two octets, else InvalidRequest.
```

### `profiles.set`

```rust
// Request: the same flattened profile shape as profiles.create — all three id
// fields optional (they identify the profile to update, with profiles.get's
// lookup semantics) — plus a force flag.
#[derive(Deserialize)]
struct ProfileSetRequest {
    #[serde(flatten)]
    profile: ProfileCreateRequest,
    /// When true, forcibly delete VPN peers that would break due to the change.
    #[serde(default)]
    force: bool,
}
// Response: ProfileId
// Validation: same gateway_ip block check as profiles.create.
```

### `profiles.delete`

```rust
// Request: ProfileIdOpt
// Response: null
```

### `profiles.edit`

CLI editor flow — opens the matched profile (or, with `create: true`, a fresh
template) in `$EDITOR`, then calls `profiles.set` / `profiles.create`. Not
called by the web UI.

```rust
#[derive(Deserialize)]
struct EditArgs {
    get: ProfileIdOpt,  // CLI: --fullname / --interface / --vlan-tag
    create: bool,       // CLI: --create
}
// Response: ProfileId
```

### `profiles.schedule-get`

```rust
#[derive(Deserialize)]
struct ScheduleGetParams {
    interface: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleWindow {
    start_time: String,  // "HH:MM"; wire: `startTime`
    end_time: String,    // "HH:MM"; wire: `endTime`
    /// [Sun, Mon, Tue, Wed, Thu, Fri, Sat]
    days: [bool; 7],
}
// Response: Vec<ScheduleWindow>
// Backend: reads the profile's wan_schedule list from UCI; errors with
// MissingProfile for an unknown interface
```

### `profiles.schedule-set`

```rust
#[derive(Deserialize)]
struct ScheduleWindows {
    interface: String,
    windows: Vec<ScheduleWindow>,
}
// Response: null
// Backend: persists the windows to the profile's UCI wan_schedule, then
// regenerates the crontab projection and firewall rules. Same wrap/overlap/
// full-week validation as wifi.blackout-set (equal startTime/endTime = full
// 24-hour window; InvalidValue on overlap or gapless full-week coverage).
```

---

## 12. SSH Keys

### `ssh-keys.list`

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SshKeyResponse {
    /// e.g. "ssh-ed25519", "ssh-rsa"
    algorithm: String,
    /// MD5 fingerprint of the public key (unique identifier)
    fingerprint: String,
    /// Comment/hostname portion
    hostname: String,
}
// Response: Vec<SshKeyResponse>
// Backend: reads /etc/dropbear/authorized_keys, parses with openssh_keys crate
```

### `ssh-keys.add`

```rust
#[derive(Deserialize)]
struct SshKeyAddRequest {
    /// Full SSH public key line (e.g. "ssh-ed25519 AAAA... user@host")
    key: String,
}
// Response: SshKeyResponse (the newly added key)
// Backend: validates with openssh_keys, checks for duplicates via fingerprint,
//          appends to /etc/dropbear/authorized_keys, creates /etc/dropbear if needed
```

### `ssh-keys.delete`

```rust
#[derive(Deserialize)]
struct SshKeyDeleteRequest {
    /// MD5 fingerprint of the key to remove
    fingerprint: String,
}
// Response: null
// Backend: removes line matching fingerprint from /etc/dropbear/authorized_keys
```

---

## 13. Setup

### `setup.status`

No auth required — polled by the setup wizard.

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupStatusRes {
    /// True when booted from removable media (setup wizard mode); wire: `setupMode`
    setup_mode: bool,
    disk: DiskState,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskState {
    /// An eMMC device was found; wire: `emmcFound`
    emmc_found: bool,
    /// The eMMC has existing firmware (rootfs partition present); wire: `hasFirmware`
    has_firmware: bool,
}
```

---

## 14. Activity

### `activity.list`

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivityListParams {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct ActivityEntry {
    id: i64,
    timestamp: String,
    category: String,   // e.g. "auth", "wan", "backup"
    action: String,     // e.g. "login", "dns-updated"
    success: bool,
    summary: String,
    /// Omitted from the JSON when absent
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityListResponse {
    entries: Vec<ActivityEntry>,
    total: usize,
}
// Backend: reads the SQLite activity log (/etc/startwrt/activity.db),
// newest first
```

### `activity.delete`

```rust
#[derive(Deserialize)]
struct ActivityDeleteParams {
    id: i64,
}
// Response: null (NotFound if no such entry)
```

### `activity.clear`

```rust
// Request: {}
// Response: null
// Backend: deletes all activity entries
```

---

## 15. Backup

### `backup.create`

```rust
// Request: {}

#[derive(Serialize)]
struct BackupCreateRes {
    /// Download the archive via GET /rest/rpc/{guid}
    guid: String,
    filename: String,  // "backup-<hostname>-<YYYY-MM-DD>.tar.gz"
}
// Backend: runs `sysupgrade --create-backup -`, buffers the archive, and
// registers a one-shot download continuation at /rest/rpc/{guid}
```

### `backup.restore`

```rust
// Request: {}

#[derive(Serialize)]
struct BackupRestoreRes {
    /// POST the .tar.gz to /rest/rpc/{guid} (10 MB body limit)
    upload: String,
}
// Backend: the upload continuation validates the archive (`tar -tzf`), applies
// it with `sysupgrade --restore-backup`, then reboots after a short delay
```

---

## 16. Diagnostics

### `diagnostics.create`

```rust
// Request: {}

#[derive(Serialize)]
struct DiagnosticsCreateRes {
    /// Download the log via GET /rest/rpc/{guid}
    guid: String,
    filename: String,  // "diagnostics-startwrt-<YYYY-MM-DD>.log"
}
// Backend: captures `logread` output and registers a one-shot download
// continuation at /rest/rpc/{guid}
```

---

## HTTP Routes

Every RPC method above is a JSON-RPC 2.0 call to a single endpoint: **`POST /rpc/v1`**.
The daemon (`backend/ctrl/src/bins/daemon.rs`) also serves:

| Route                                              | Method    | Auth                       | Purpose                                                                                      |
| -------------------------------------------------- | --------- | -------------------------- | -------------------------------------------------------------------------------------------- |
| `/rpc/v1`                                          | POST      | Session (unless `no_auth`) | JSON-RPC 2.0 endpoint for all RPC methods                                                    |
| `/rest/rpc/{guid}`                                 | GET/POST  | GUID capability (one-shot) | RPC continuation: binary download (backup, diagnostics) / upload (restore); 10 MB body limit |
| `/ws/rpc/{guid}`                                   | WebSocket | GUID capability            | Progress streaming (`system.update`)                                                         |
| `/api/logs`                                        | WebSocket | Session or local cookie    | Live log streaming (see § 2)                                                                 |
| `/api/setup/flash`                                 | POST      | None (setup wizard)        | Streams NDJSON `SetupEvent` progress while flashing the eMMC; one flash at a time            |
| `/static/root-ca.crt`                              | GET       | None                       | Root CA certificate download                                                                 |
| `/cgi-bin/*`, `/luci-static/*`, `/ubus`, `/ubus/*` | any       | LuCI's own                 | Reverse proxy to uhttpd (LuCI) on localhost:8080; `/luci` redirects to `/cgi-bin/luci`       |
| everything else                                    | any       | None                       | Embedded web UI                                                                              |

---

## Endpoint Summary

| RPC Method                   | Category        | Notes                      |
| ---------------------------- | --------------- | -------------------------- |
| `auth.login`                 | Auth            | Rate-limited               |
| `auth.logout`                | Auth            |                            |
| `auth.verify-password`       | Auth            |                            |
| `auth.set-password`          | Auth            |                            |
| `auth.check-initialized`     | Auth            | No auth                    |
| `auth.set-initial-password`  | Auth            | No session; rate-limited   |
| `system.info`                | System          | No auth                    |
| `system.newer-versions`      | System          | No auth                    |
| `system.update`              | System          |                            |
| `system.restart`             | System          |                            |
| `system.factory-reset`       | System          |                            |
| `system.set-preferences`     | System          |                            |
| `system.apply-remote-access` | System          | No auth; internal, hotplug |
| `system.set-timezone`        | System          | No auth                    |
| `system.get-timezones`       | System          | No auth                    |
| `system.logs`                | System          |                            |
| `setup.status`               | Setup           | No auth                    |
| `wan.ipv4-get`               | WAN             |                            |
| `wan.ipv4-set`               | WAN             |                            |
| `wan.ipv6-get`               | WAN             |                            |
| `wan.ipv6-set`               | WAN             |                            |
| `wan.mac-get`                | WAN             |                            |
| `wan.mac-set`                | WAN             |                            |
| `wan.dns-get`                | WAN             |                            |
| `wan.dns-set`                | WAN             |                            |
| `wan.ddns-get`               | WAN             |                            |
| `wan.ddns-set`               | WAN             |                            |
| `lan.ipv4-get`               | LAN             |                            |
| `lan.ipv4-set`               | LAN             |                            |
| `lan.ipv6-get`               | LAN             |                            |
| `lan.ipv6-set`               | LAN             |                            |
| `ethernet.get`               | Ethernet        |                            |
| `ethernet.set`               | Ethernet        |                            |
| `ethernet.edit`              | Ethernet        | CLI editor                 |
| `devices.list`               | Devices         |                            |
| `devices.update`             | Devices         |                            |
| `devices.forget`             | Devices         |                            |
| `devices.data-usage`         | Devices         |                            |
| `published-ports.list`       | Published Ports |                            |
| `published-ports.set`        | Published Ports |                            |
| `published-ports.reconcile`  | Published Ports | No auth; internal, hotplug |
| `vpn-client.list`            | Outbound VPN    |                            |
| `vpn-client.create`          | Outbound VPN    |                            |
| `vpn-client.update`          | Outbound VPN    |                            |
| `vpn-client.delete`          | Outbound VPN    |                            |
| `vpn-client.set-enabled`     | Outbound VPN    |                            |
| `vpn-server.list`            | Inbound VPN     |                            |
| `vpn-server.set`             | Inbound VPN     |                            |
| `vpn-server.delete`          | Inbound VPN     |                            |
| `vpn-server.peer-add`        | Inbound VPN     |                            |
| `vpn-server.peer-delete`     | Inbound VPN     |                            |
| `wifi.get`                   | WiFi            |                            |
| `wifi.set`                   | WiFi            |                            |
| `wifi.edit`                  | WiFi            | CLI editor                 |
| `wifi.blackout-get`          | WiFi            |                            |
| `wifi.blackout-set`          | WiFi            |                            |
| `wifi.generate-password`     | WiFi            |                            |
| `profiles.list`              | Profiles        |                            |
| `profiles.get`               | Profiles        |                            |
| `profiles.create`            | Profiles        |                            |
| `profiles.set`               | Profiles        |                            |
| `profiles.delete`            | Profiles        |                            |
| `profiles.edit`              | Profiles        | CLI editor                 |
| `profiles.schedule-get`      | Profiles        |                            |
| `profiles.schedule-set`      | Profiles        |                            |
| `ssh-keys.list`              | SSH Keys        |                            |
| `ssh-keys.add`               | SSH Keys        |                            |
| `ssh-keys.delete`            | SSH Keys        |                            |
| `activity.list`              | Activity        |                            |
| `activity.delete`            | Activity        |                            |
| `activity.clear`             | Activity        |                            |
| `backup.create`              | Backup          |                            |
| `backup.restore`             | Backup          |                            |
| `diagnostics.create`         | Diagnostics     |                            |

**Totals:** 74 RPC methods across 16 categories, plus the HTTP/WebSocket routes
table above and the deprecated generic endpoints below.

---

## Deprecated Endpoints

The following generic endpoints should be removed from the frontend once all smart endpoints are implemented:

| Endpoint   | Replaced by                                                                       |
| ---------- | --------------------------------------------------------------------------------- |
| `uci.get`  | All `*.get` endpoints above                                                       |
| `uci.set`  | All `*.set` endpoints above                                                       |
| `uci.edit` | The `*.edit` CLI editors above (editor over `uci.get` + `uci.set`)                |
| `exec`     | Absorbed into smart endpoints internally                                          |
| `file.get` | `ssh-keys.list`                                                                   |
| `file.set` | `ssh-keys.add`, `ssh-keys.delete`                                                 |
| `dir.get`  | Absorbed into smart endpoints internally (directory stat/listing; CLI/debug only) |
