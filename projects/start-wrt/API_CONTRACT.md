# StartWRT API Contract

Complete RPC API contract for the StartWRT backend. All endpoints use **JSON-RPC 2.0** over a single HTTP POST endpoint. Request and response types are defined in Rust.

**Goal:** The frontend should never touch UCI files, run shell commands, or read raw files. Every operation goes through a purpose-built RPC method. The backend handles all UCI manipulation, service restarts, and system queries internally.

Every `/rpc/v1` response carries an `x-startwrt-git-hash` header with the firmware's build stamp (full git hash, `-modified` suffix on dirty builds — same value as `system.info`'s `gitHash`, exposed through CORS). The UI compares it against its baked-in `config.json` gitHash on every response, so an open tab detects a firmware update from its next request (including the periodic background form polls) even when the daemon restart was too quick to drop a connection.

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
```

---

## 1. Auth

A request is authorized if either credential is valid:

- **Request signature** — an `X-Start-Auth-Sig` header carrying an Ed25519 signature, by a key enrolled through `auth.login`, over a timestamp, a nonce, the body's BLAKE3 hash, and the server identity the client addressed (`router.lan`, any non-loopback address on the router's interfaces, or the enabled DDNS hostname; a loopback address only from a loopback peer). The protocol is `start-core`'s `SignatureAuth`; the browser holds a non-extractable WebCrypto key in IndexedDB. Timestamps must fall within 30 seconds of the router's clock once ntpd reports valid time; a replayed nonce is rejected.
- **Local auth token** — `Authorization: Bearer <token>`, where the token is the contents of `/run/startwrt/rpc.authcookie` (regenerated at daemon start, mode `0640`, `root:root`). `startwrt-cli` sends it only to loopback hosts.

Endpoints registered with `authenticated: false` metadata answer without either. Enrolled keys persist in `/etc/startwrt/auth.db`, which sysupgrade keeps, and stay valid until revoked by `auth.logout`, `auth.session.kill`, or a password change, or until 24 hours after their `loggedIn` time, however recently they were used. The microSD wizard's Keep settings reflash does not carry them over. An expired key is rejected as unauthorized (code 34) on its next request. Before ntpd reports valid time the check judges age alone; afterwards a `loggedIn` more than 5 minutes in the future also counts as expired. The sweep that unenrolls expired keys runs hourly and waits for ntpd to report valid time.

### `auth.login`

The request must be signed by the key it enrolls. A valid local auth token rejects the call as unnecessary. Before first-time setup sets a password, every login fails with Uninitialized (code 40).

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginParams {
    password: String,
    /// PEM-encoded Ed25519 public key to enroll
    pubkey: AnyVerifyingKey,
    /// Enroll in memory only. Unsupported (InvalidRequest).
    #[serde(default)]
    ephemeral: bool,
}
// Response: null
// Rate-limited: 3 attempts per 20 seconds per signed server identity.
```

### `auth.logout`

Revokes the key that signed the request and closes the streams it opened. An
unsigned local-token request is rejected as unnecessary.

```rust
// Request: {} (empty object)
// Response: null
```

### `auth.session.list`

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionList {
    /// The key that signed this request
    current: Option<String>,
    /// Enrolled keys by PEM: { name, loggedIn, lastActive, userAgent }.
    /// `lastActive` is persisted at most hourly.
    sessions: BTreeMap<String, Session>,
}
```

### `auth.session.kill`

```rust
#[derive(Deserialize)]
struct KillParams {
    /// PEM-encoded keys to revoke
    ids: Vec<String>,
}
// Response: null
```

### `auth.set-password`

Once the new hash is written, revokes every enrolled key except the one that
signed the request, and closes the streams they opened. A local-token request
revokes every key. A failed write revokes none. Fails with Uninitialized
(code 40) before first-time setup.

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

Verifies the admin password without enrolling a key (used internally by the
CLI before prompting for a new one).

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifyPasswordParams {
    password: String,
}
// Response: null (IncorrectPassword, code 7, on a wrong password;
// Uninitialized, code 40, before first-time setup)
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

First-time setup: no auth required. Rejected once a password is already set.
Enrolls nothing — the setup page runs over plain HTTP, where a browser cannot
sign, so the user logs in afterwards over HTTPS.

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetInitialPasswordParams {
    /// Minimum 12 characters (same rule as auth.set-password)
    password: String,
    /// IANA timezone name, e.g. "America/New_York". An unknown name is skipped.
    timezone: Option<String>,
}
// Response: null
// Backend: writes the root hash to /etc/shadow, revokes every enrolled key,
// applies the timezone, disables the captive portal
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
    git_hash: String,  // firmware build stamp (full git hash, "-modified" suffix on dirty builds);
                       // the UI compares this against its baked-in config.json gitHash to detect
                       // a stale cached bundle and prompt/perform a reload
    language: String,
    date: String,  // ISO 8601
    theme: String,  // "dark" | "light" | "system"
    remote_access: String,  // wire: `remoteAccess`; "default" | "never" | "always"
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
struct SetPreferencesReq {
    language: Option<String>,  // "en_US" | "es_ES" | "de_DE" | "fr_FR" | "pl_PL"
    theme: Option<String>,  // "dark" | "light" | "system"
    remote_access: Option<String>,  // wire: `remoteAccess`; "default" | "never" | "always"
}
// Response: null
// A value outside its set is rejected with InvalidValue.
```

### `system.apply-remote-access`

Internal endpoint, **not called from the frontend**. `startwrt-cli` runs it in
its own process (it is not registered with `with_call_remote`), so the hook
never reaches the daemon. Fired by the
`/etc/hotplug.d/iface/99-startwrt-remote-access` hook (in `backend/hotplug/`)
on `ifup`, `ifupdate`, and `ifdown` of `wan` or `wan6`.

```rust
// Request: {}
// Response: null
// Backend: re-applies the remote-access preference to the firewall against the
// current WAN IPs, reloads the firewall, and kills WAN-side dropbear (SSH)
// sessions from sources the mode no longer admits, so they don't survive a rule
// change via conntrack ESTABLISHED state
```

### `system.set-timezone`

Auth required. First-time setup passes its timezone to
`auth.set-initial-password` instead.

```rust
#[derive(Deserialize)]
struct SetTimezoneParams {
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

### `system.logs-follow`

Live-streaming endpoint for the web UI. Not exposed to the CLI.

```rust
// Request: {}

#[derive(Serialize)]
struct FollowLogsRes {
    guid: Guid,
}
// Response: FollowLogsRes
```

1. Client opens a WebSocket to `/ws/rpc/{guid}` within 30 seconds; the GUID is single-use
2. Server spawns `logread -f` (dumps historical entries then follows new ones)
3. Each line is parsed into `LogEntry` and sent as a JSON text frame
4. Connection closes when either side disconnects, or when the key that requested it is revoked; child process is killed on drop
5. Each message is a single `LogEntry` JSON object (not wrapped in `LogsResponse`)
6. Unparseable lines are silently dropped (same as the RPC endpoint)

---

## 3. WAN

### `wan.ipv4-get`

Returns WAN IPv4 configuration, the actual assigned IP, and whether the WAN port has link.

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
    /// Whether the WAN port has link (from ubus `network.device` status). A failed read reports link.
    carrier: bool,
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
    /// A wan6 proto startwrt doesn't manage (e.g. a hand-configured `6in4`)
    /// is reported as Disabled, not an error; the config is left untouched.
    /// A disabled wan6 still reports the settings it keeps.
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
    /// SLAAC/DHCPv6: requested delegation length, e.g. "/60"; unset or empty requests auto
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
// Backend: updates UCI network.wan6, restarts network, then restarts odhcpd.
// Disabled sets `auto 0` on wan6, keeping its other options; any other mode clears it.
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
    /// The WAN device's current MAC from `ip link` (the custom MAC while one is applied)
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
// Backend: sets (custom) or clears (router) `macaddr` on the `config device`
// section named by `network.wan`'s device, creating it if needed; restarts network
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

A UCI section with an unknown `service_name` (e.g. `start9`, whose service
never launched) reads back as the disabled default (`enabled: false`,
`provider: dyndns`, all fields null). The legacy `freedns.afraid.org` reads back
as `freedns`.

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
// Backend: updates UCI network.lan ipaddr (netmask /24) — and every profile's
// addresses on a /16 block change — then clears DHCP leases, cycles only the LAN
// interface (plus every profile interface on a block change) with ifdown/ifup,
// reloads the firewall and dnsmasq, and restarts Wi-Fi.
// Inbound VPN peers the change would break fail the call with
//   ErrorKind::VpnPeersWouldBreak unless force is set.
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
//   Changing the address while a DHCP reservation sits in the old /24 (the old
//   /16 on a block change) is rejected with ErrorKind::DhcpStaticHostsInSubnet.
```

### `lan.ipv6-get`

```rust
// Request: {}

#[derive(Serialize)]
struct LanIpv6Response {
    slaac: bool,
    dhcpv6: bool,
    /// LAN prefix length in effect: the configured length, lengthened to the
    /// delegated prefix when shorter.
    prefix: u8,
    /// Current IPv6 address (if assigned)
    ip6addr: Option<String>,
    /// Length of the global prefix the ISP delegated, read from netifd's
    /// wan6 status; null without one.
    delegated_prefix: Option<u8>,
}
```

### `lan.ipv6-set`

```rust
#[derive(Deserialize)]
struct LanIpv6SetRequest {
    slaac: bool,
    dhcpv6: bool,
    /// Absent keeps the configured prefix length (64 when none is set).
    #[serde(default)]
    prefix: Option<u8>,
}
// Response: null
// With SLAAC enabled, `prefix` outside 48..=64, or shorter than the live
// delegated prefix, is rejected (`InvalidRequest`), matching the UI validator.
// Backend: updates UCI dhcp + network for the LAN and every profile, reloads
// odhcpd to withdraw the old prefix, and restarts network.
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
    /// live mDNS name → remembered hostname (name cache) → derived label
    /// (OS from the DHCP fingerprint, e.g. `Windows device (b2c3d4)`, else
    /// vendor from the MAC's OUI, e.g. `Apple device (b2c3d4)`) →
    /// `device-<mac>` placeholder. Always set.
    name: String,
    /// The name assigned in the router (the UCI static name); `None` when
    /// `name` is resolved from elsewhere. What the rename form edits.
    custom_name: Option<String>,
    /// Raw DHCP lease hostname ("*" when unset).
    hostname: Option<String>,
    status: DeviceStatus,
    /// "Ethernet", "Wi-Fi 2.4GHz", "Wi-Fi 5GHz", etc.
    connection: Option<String>,
    ipv4: Option<String>,
    ipv6: Option<String>,
    ipv4_static: bool,
    /// Whether this device may auto-create port forwards via PCP/UPnP
    /// (default off; toggled via `devices.set-auto-forward`). Always false for
    /// VPN peers (no MAC to authorize).
    allow_auto_port_forward: bool,
    /// Whether this device may publish DNS records into the router's resolver
    /// (`devices.set-dns-injection`; `vpn-server.peer-set-dns-injection` for a
    /// VPN peer).
    allow_dns_injection: bool,
    /// Whether the router forwards this device's IPv6 router advertisements
    /// to the rest of the LAN (`devices.set-router-advertisements`). Always
    /// false for VPN peers.
    allow_router_advertisements: bool,
    security_profile: Option<String>,
    /// Live throughput (MB/s, 1 decimal), computed from conntrack byte deltas
    /// between polls (`wg show` byte deltas for a VPN peer). Only set for online
    /// devices with a previous sample.
    speed: Option<SpeedData>,
    /// Total data used in the current nlbwmon period (GB, 1 decimal, rx+tx); for
    /// a VPN peer, its cumulative `wg show` rx+tx since the interface came up
    data_usage: Option<f64>,
}

#[derive(Serialize)]
struct SpeedData {
    up: f64,
    down: f64,
}
// Response: Vec<Device>
// Backend: reads DHCP hosts, the neighbor table, the bridge FDB, hostapd
// clients, DHCP leases, nlbwmon, conntrack, `wg show`, a
// persistent identity cache (/etc/startwrt/device_names.json) that remembers
// DHCP/mDNS-advertised hostnames and DHCP fingerprints per MAC, live DHCP
// fingerprints captured by a dnsmasq dhcp-script hook
// (/var/run/dnsmasq/dhcp.fingerprints), and an embedded IEEE OUI registry
// snapshot — the last two label devices that never advertise a name. The
// backend resolves the full name fallback chain server-side and returns a
// single `name`.
```

### `devices.update`

```rust
#[derive(Deserialize)]
struct DeviceUpdateReq {
    /// `AA:BB:CC:DD:EE:FF`.
    mac: String,
    /// Absent leaves the assigned name untouched; empty clears it. Otherwise
    /// a hostname label — letters, digits, and hyphens, no leading or
    /// trailing hyphen, at most 63 characters — since dnsmasq serves it.
    #[serde(default)]
    name: Option<String>,
    ipv4_static: bool,
    /// Dotted-quad, or empty for no reservation.
    ipv4: String,
}
// Every field lands in the config dnsmasq reads, which it refuses to start on
// if malformed, so each is validated: a bad one is rejected with
// `InvalidValue` and nothing is written.
// Response: null
// Backend: creates/updates DHCP host section, reloads dnsmasq.
// No IPv6 fields: devices choose their own IPv6 addresses (SLAAC), so there is
// no user-facing IPv6 reservation. A legacy `hostid` (written by older releases,
// read by `published-ports.reconcile` as a fallback) is left untouched.
```

### `devices.set-auto-forward`

```rust
#[derive(Deserialize)]
struct SetAutoForwardRequest {
    mac: String,
    allow: bool,
}
// Response: null
// Backend: stores the flag on the device's DHCP host section (`_allow_pcp`),
// creating one if needed, and invalidates the port-control authorization cache
// so the change applies immediately. Default is off: a device with no flag can
// never create forwards via PCP/UPnP. Setting `allow: false` also closes the
// forwards the device already holds, rather than leaving them open until their
// leases lapse (up to a week).
```

### `devices.set-dns-injection`

```rust
#[derive(Deserialize)]
struct SetDnsInjectionReq {
    mac: String,
    allow: bool,
}
// Response: null
// Backend: stores `_allow_dns_inject` on the device's DHCP host section,
// creating one if needed, rewrites the per-profile dnsmasq instances, and
// applies the change immediately. `allow: false` also drops the records the
// device has published.
```

### `devices.set-router-advertisements`

```rust
#[derive(Deserialize)]
struct SetRouterAdvertisementsReq {
    mac: String,
    allow: bool,
}
// Response: null
// Backend: stores `_allow_ra` on the device's DHCP host section, creating one
// if needed, renders the allowed MACs into the LAN guard's `ra_allow` set
// (/usr/share/nftables.d/ruleset-pre/11-startwrt-lan-ra-allow.nft), and
// reloads the firewall on every call, including one that changes nothing, so
// repeating a request whose reload failed applies it. A failed reload is an
// error even though the setting is saved. Default is off: the guard drops
// router advertisements switched between bridge ports from any other source
// MAC.
```

### `devices.forget`

```rust
#[derive(Deserialize)]
struct DeviceMacReq {
    mac: String,
}
// Response: null
// Backend: removes the DHCP host for the MAC and its remembered name, closes
// any automatic (PCP/UPnP) forwards the device holds — forgetting it drops the
// `_allow_pcp` flag with the host section, so those forwards would otherwise
// outlive the authorization that created them — removes it from the LAN
// guard's router-advertisement allow list, flushes
// matching neighbor/ARP entries (`ip neigh del`), and rewrites every dnsmasq
// lease file (the base file plus per-profile /tmp/dhcp.leases.dns_*) with each
// instance holding the lease stopped by name, so the in-memory lease can't
// resurrect the entry, then reloads dnsmasq. A still-connected device reappears
// on its next network activity. If the allow-list reload fails, the rest still
// happens and the call returns the error; forgetting again retries it.
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
struct DataUsageReq {
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
    /// One protocol works but the other can't: the device has no IPv4 ("IPv4
    /// unavailable") or no IPv6 ("IPv6 unavailable") address, or the IPv6
    /// forward is stranded on an old prefix ("IPv6 address out of date").
    Partial,
    /// Device offline, or not in the device list ("Device not found")
    Paused,
    /// No usable address ("No addresses available"), or (IPv6-only) the forward
    /// is stranded on an old delegated prefix ("IPv6 address out of date" — the
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
    /// The user confirmed capturing a WAN port already in use (see
    /// `published-ports.set`); round-tripped so later saves don't re-prompt.
    override_wan_ports: bool,
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
    /// Confirms forwarding a WAN port already used by a router service or
    /// hostname route. Round-tripped through `list` so confirmation is asked
    /// once per port.
    #[serde(default)]
    override_wan_ports: bool,
}

#[derive(Deserialize)]
struct PublishedPortsSetRequest {
    ports: Vec<PublishedPortInput>,
}

#[derive(Serialize)]
struct WanPortCollision {
    id: String,
    label: String,
    /// The colliding router-service port spec(s), e.g. ["443", "22"].
    router_service_ports: Vec<String>,
    /// Colliding ports held by hostname routes.
    hostname_route_ports: Vec<SniPortUse>,
}

#[derive(Serialize)]
struct SniPortUse {
    ports: String,
    /// The routed hostnames on the port, deduped and sorted.
    hostnames: Vec<String>,
    /// Display names (or MACs) of the devices the routes deliver to.
    devices: Vec<String>,
}

#[derive(Serialize)]
struct PublishedPortsSetResult {
    pending_wan_port_collisions: Vec<WanPortCollision>,
}
// Backend: rebuilds firewall redirect+rule sections (dropping automatic forwards
// that overlap an enabled manual port), resolves device IPs, adds a static DHCP
// reservation for an enabled IPv4 port's device that has none, then reloads the
// firewall (restarting it only if the reload fails) and, if a reservation was
// added, dnsmasq
```

The request is applied **unless** an enabled IPv4 forward without
`override_wan_ports` would capture a port already in use on the WAN — a live input-chain ACCEPT rule: Remote Access (80/443/22, including
"When behind NAT" mode whenever the WAN address is private) or the VPN
server's listen port. nftables applies prerouting DNAT before the routing
decision, so such a forward silently diverts those router services to the
device (issue #3451). In that case nothing is applied and
`pending_wan_port_collisions` names the offending ports; the UI shows a
confirmation dialog and re-saves with `override_wan_ports: true` on the named
ports. An empty list in the response means the request was applied.
A colliding port whose WAN-input rule is an SNI-demux admit rule is reported
under `hostname_route_ports` instead of `router_service_ports`, with the routed
hostnames and owning devices filled in from the live demux — the real holder is a device's
hostname routes, and the dialog names them rather than blaming the router.
The split is informational only: the override works identically and removes
the displaced hostname routes when the manual rule is saved.
Detection is transport-aware (Remote Access is TCP, WireGuard is UDP — a
UDP-only forward on 443 collides with nothing) and skipped in configs-only
mode (the CLI editor confirms implicitly, like `ethernet.set` / `wifi.set`).
The automatic (PCP/UPnP) path refuses the same collision outright — a
protocol client can't be asked.

Validation (errors with `MissingDeviceAddress`, rejecting the whole request):

- An enabled `ipv4` port whose device has no resolvable IPv4 address.
- An enabled `ipv6` port when the router has no global IPv6 prefix delegated.
- An enabled `ipv6` port whose device has no GUA — it is offline, or has only a
  ULA or link-local address — unless the port already has an IPv6 rule for the
  same `id` and `device_mac`. A new IPv6 port, one newly switched to IPv6, or
  one moved to another device needs the device online with a GUA.

An `ipv6` port's rule pins the tracker-elected stable GUA the device holds
within a currently-assigned prefix; with none (e.g. saved mid-prefix-rotation),
the resolved address is used as-is and the tracker's reconcile retargets the
rule within seconds of the device acquiring an in-prefix address. With neither
(the device has no GUA), the rule keeps the `dest_ip` its existing rule for the
same `id` and `device_mac` held, as an fw4 rule keeps its address while the
host is away, and keeps that rule's `dest` zone when the device's zone cannot
be resolved either; this holds for disabled ports and in configs-only mode too. A
disabled port, or any port in configs-only mode (neither is validated), with no
such rule and no address writes no IPv6 rule and reads back with `ipv6: false`.
There is no DHCP `hostid` pinning (devices choose their own IPv6 addresses; odhcpd also
misparses colon-separated hostids) — `reconcile` keeps the rule current
instead. Legacy hostids written by older releases are left in place and still
serve as a last-resort reconcile fallback.

### `published-ports.auto-list`

Automatic port uses created by authorized LAN devices themselves via PCP or
UPnP IGD (the server half of StartOS's gateway autoconfiguration). Read-only from
the UI: the requesting device creates, renews, and withdraws them; unrenewed
forwards expire on the lifetime the protocol granted the client (the daemon
sweeps leases every minute). Distinct from manual published ports — they never
appear in `published-ports.list`, and `published-ports.set` leaves them alone
_except_ that a manual rule claiming an auto-held external port wins: `set`
removes the overlapping auto forward.

Besides plain forwards, the list includes **SNI hostname routes** (label
`"SNI"`, `hostname` set): TLS routes on a shared external port, demultiplexed
by ClientHello SNI so several devices/hostnames share one port. These live in
daemon memory (not UCI), always carry a lease (≤1h, device-renewed), and do
not survive a daemon restart — the device re-asserts them. A demuxed port
reads as router-reserved to manual and plain-auto forwards. Ports the router
answers on itself refuse hostname routes — except 443, where Remote Access
coexists with the demux: while routes share the port, connections naming no
routed hostname (e.g. browsing the router by IP, which sends no SNI) are piped
to the router's own UI, admitted from exactly the sources the Remote Access
mode allows (any in "always"; RFC1918 in "default" behind NAT; none in
"never"). A route is granted only once its listener is actually bound
(`NO_RESOURCES` / UPnP fault 501 otherwise), so a granted route can never
leave the port open with nothing serving it.

```rust
// Request: {}

#[derive(Serialize)]
struct AutomaticPortUse {
    /// Stable per automatic port use.
    id: String,
    /// "PCP", "UPnP", or "SNI" for a hostname route.
    kind: String,
    /// May be empty on an SNI row whose target address matches no known device.
    device_mac: String,
    device_name: Option<String>,
    /// Forward target address on the LAN.
    internal_ip: Option<String>,
    /// Internal port or range, e.g. "8443" or "1000-1009".
    ports: String,
    /// External (WAN) port or range.
    public_ports: String,
    /// Seconds until the lease expires if the device stops renewing it. None
    /// right after a daemon restart, before the first grace lease is granted.
    expires_secs: Option<u64>,
    /// TLS-SNI hostname for an SNI route; None for plain forwards.
    hostname: Option<String>,
}
// Response: Vec<AutomaticPortUse>
// Backend: reads automatic firewall redirects plus the SNI demux's live
// routes; names enriched from DHCP host entries and the persistent
// device-name cache.
```

### `published-ports.wan-changed`

```rust
// Request: {}
// Response: null
```

Internal endpoint, **not called from the frontend**; its `startwrt-cli` caller
authenticates with the local auth token. Fired by the
`/etc/hotplug.d/iface/99-startwrt-port-control` hook on `wan` `ifup`/
`ifupdate`: forwards to the daemon, which re-keys live SNI hostname routes
onto the (possibly changed) WAN IPv4 — their listeners bind the WAN address
itself — and re-scopes the HTTP→HTTPS redirect's port-80 WAN admission rule
(`HTTP to HTTPS redirect`, present while WAN TCP 443 is published) to that
address. The daemon's sweep re-checks once a minute as a backstop. No-op in
configs-only mode.

### `published-ports.reconcile`

```rust
// Request: {}
// Response: null
```

Internal endpoint, **not called from the frontend**; its `startwrt-cli` caller
authenticates with the local auth token. Fired two
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

### `published-ports.sync-hairpin`

```rust
// Request: {}
// Response: null
```

Internal endpoint, **not called from the frontend**. Re-derives every hairpin
projection from the firewall config and the router's current WAN IPv4
addresses (`ubus call network.interface.wan status`) — the `reflection_zone`
list on each published-port and automatic-forward redirect, and the LAN-side
copies of each IPv6 published-port rule — and writes the config only if
something changed. A profile's WAN Whitelist and Blacklist entries are read
against those addresses (for an IPv6 rule, against the device's own address),
so the result depends on the live WAN state. It does not reload the firewall:
the WAN-schedule crontab runs it between its `uci commit` and its
`/etc/init.d/firewall reload`, where the blackout REJECT the edge just wrote
must also take that profile's hairpin away, and the `wan` hotplug hook
(`99-startwrt-published-ports`) runs it on `ifup`/`ifupdate` followed by its
own reload. Runs in the CLI process against `/etc/config` (no
`with_call_remote`).

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
    /// True when the peer Endpoint is a hostname. Such a VPN can only target
    /// "Internet"; the web UI offers no other target for it.
    hostname_endpoint: bool,
    /// Resolvers reached through the tunnel (the interface's `list dns`).
    dns: Vec<IpAddr>,
    status: OutboundVpnStatus,
    /// Seconds since the tunnel's latest handshake, measured on the router.
    /// null when it has never completed one.
    last_handshake_secs: Option<u64>,
}

#[serde(rename_all = "kebab-case")]
enum OutboundVpnStatus {
    Disabled,
    /// A handshake completed within the last 180 seconds.
    Connected,
    /// Nothing has sent through the tunnel since its session lapsed.
    Idle,
    /// Sends go unanswered: the peer never answered a second handshake
    /// initiation, or the router sent after the session lapsed within the last
    /// 120 seconds. Also an enabled tunnel with no peer endpoint.
    NotResponding,
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
    /// Replaces the .conf's `DNS =` servers. null/absent keeps them.
    #[serde(default)]
    dns: Option<Vec<IpAddr>>,
}

#[derive(Serialize)]
struct OutboundVpnCreateResponse {
    /// The assigned interface ID
    id: String,
}
// Backend: parses WireGuard config, creates UCI interface+peer, bounces the WG
// interface, and reloads network.
// An uncommented `MTU` in the .conf (1280–1500) becomes `option mtu`; a
// commented `#MTU` is ignored. The dedicated vpn_<X> egress zone also gets
// `option mtu_fix 1` (TCP MSS clamp) as a backstop against too-high MTU.
// The address entries of the .conf's `DNS =` line (or `dns`, when given)
// become `list dns`; search-domain entries are dropped.
//
// DNS for a profile routed through the VPN, first match wins: the profile's
// custom servers, then the VPN's `list dns`, then the system custom servers —
// all reached through the tunnel and failing closed with it (dnsmasq
// `server <ip>@<iface>` for the VPN's own servers, a per-profile SmartDNS group
// with `-interface <iface>` upstreams otherwise). With none of them, the
// profile resolves through the ISP's servers outside the tunnel and reports
// `dns_source = "system-outside-vpn"`.
//
// Validation (InvalidValue on failure): the config MUST be a WireGuard .conf —
// it requires `[Interface]` + `[Peer]` headers, a valid PrivateKey/PublicKey
// (base64 32-byte), at least one interface `Address`, and a peer `Endpoint`.
// OpenVPN configs (no INI headers) are rejected with an OpenVPN-specific
// message. The web UI mirrors this with an async file-content check before
// submit, but the backend is the authoritative gate.
//
// `target` must be "Internet" or an existing VPN's label (else InvalidValue),
// must not close a chain cycle (VpnChainCycle), and must not name a VPN that
// is disabled (VpnDisabled) — a disabled interface is never registered with
// netifd, so the vcr_<iface> chain route is dropped and the vcrb_<iface>
// fallback leaves this VPN's endpoint unreachable. `vpn-client.set-enabled` holds the
// other half of that invariant, refusing to disable a VPN that something
// already chains through.
//
// A chained VPN (target ≠ "Internet") additionally needs (InvalidValue):
//   * an IP-literal Endpoint — only an address can be routed through the target.
//     At boot, a chained VPN with a hostname Endpoint is retargeted to
//     "Internet" and the change logged to activity (`vpn-client.unchained`);
//   * a target tunnel with an address of the endpoint's family whose peer
//     AllowedIPs cover the endpoint;
//   * an endpoint address no other VPN shares (checked for every VPN, since a
//     chain route captures all router traffic to that address).
// A bracketed IPv6 Endpoint is stored without its brackets.
//
// Routing: each chained VPN gets `vcr_<iface>` (endpoint /32 or /128 → target
// tunnel, main table) plus `vcrb_<iface>`, an `unreachable` route on loopback
// at metric 2048, so the endpoint is unreachable rather than reached over the
// WAN while the target is down. A routed chained VPN's interface also gets
// `nohostroute '1'`: netifd's endpoint host route would otherwise copy the
// fallback as a unicast route over it. With no MTU in the .conf, a chained
// VPN gets the chain MTU: the target's MTU (default 1420) less 60 (IPv4
// endpoint) or 80 (IPv6), floored at 1280.
```

### `vpn-client.update`

```rust
#[derive(Deserialize)]
struct OutboundVpnUpdateRequest {
    id: String,
    label: String,
    target: String,
    /// Desired interface MTU (1280–1500). null/absent restores the default:
    /// the chain MTU for a chained VPN, else the kernel's. UCI is the single
    /// source of truth — there is no stored .conf; the web edit form always
    /// submits the field's current value.
    #[serde(default)]
    mtu: Option<u16>,
    /// Replaces the interface's `list dns`. null/absent keeps it.
    #[serde(default)]
    dns: Option<Vec<IpAddr>>,
}
// Response: null
// Backend: updates label+target metadata and the interface `mtu` and `dns`
// options. Bounces the WG interface only when the MTU actually changed; a DNS
// change rewrites the DNS forwarding of the profiles routed through it.
//
// Validation: same `target` rules as vpn-client.create, except the disabled
// and chained-endpoint checks run ONLY when `target` differs from the stored
// one — mirroring guard_subnet_collision, so an unrelated edit (label, MTU)
// isn't blocked by a broken chain already present in the config. The cycle
// check runs after a rename is carried into dependents' targets.
```

### `vpn-client.delete`

```rust
#[derive(Deserialize)]
struct OutboundVpnDeleteRequest {
    id: String,
}
// Response: null
// Backend: removes UCI interface+peer sections and reloads network. Fails with
// VpnHasDependents while another VPN chains through it; profiles routed through
// it revert to `wan`.
```

### `vpn-client.set-enabled`

```rust
#[derive(Deserialize)]
struct OutboundVpnSetEnabledRequest {
    id: String,
    enabled: bool,
}
// Response: null
// Backend: sets/clears UCI disabled flag, brings the WG interface up/down, and
// reloads network. Disabling fails with VpnHasDependents while another VPN
// chains through it, and reverts profiles routed through it to `wan`.
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
    /// May publish DNS records into the router's resolver. Default/absent = denied.
    allow_dns_injection: Option<bool>,
}

#[derive(Serialize)]
struct VpnServers {
    servers: Vec<VpnServer>,
}
```

### `vpn-server.set`

```rust
#[derive(Deserialize)]
struct SetArgs {
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
struct DeleteArgs {
    profile: String,
}
// Response: null
```

### `vpn-server.peer-add`

```rust
#[derive(Deserialize)]
struct PeerAddArgs {
    profile: String,
    peer: VpnServerPeer,  // the vpn-server.list peer shape
}

#[derive(Serialize)]
struct PeerAddResponse {
    /// WireGuard client config; `PrivateKey = <PRIVATE_KEY>` when the request supplied `public_key`
    client_config: String,
    public_key: String,
    ip: String,
}
```

### `vpn-server.peer-delete`

```rust
#[derive(Deserialize)]
struct PeerDeleteArgs {
    profile: String,
    public_key: String,
}
// Response: null
```

### `vpn-server.peer-rename`

Changes only the peer's display name. The key, address, and WireGuard session are untouched.
Fails with `NotFound` if the server or the peer does not exist.

```rust
#[derive(Deserialize)]
struct PeerRenameArgs {
    profile: String,
    public_key: String,
    /// Validated like `peer-add`'s `name`.
    name: String,
}
// Response: null
```

### `vpn-server.peer-set-dns-injection`

```rust
#[derive(Deserialize)]
struct PeerSetDnsInjectionArgs {
    profile: String,
    public_key: String,
    allow: bool,
}
// Response: null
// Backend: stores `_allow_dns_inject` on the peer's `wireguard_wg_<profile>`
// section, rewrites the per-profile dnsmasq instances, and applies the change
// immediately. A peer's signed updates are refused without it. `allow: false`
// also drops the records the peer has published.
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
    /// Channel width in MHz, from the radio's htmode. Null when the mode sets
    /// none (NOHT); null on `set` keeps the current width.
    width: Option<u32>,
    enabled: bool,
    broadcast: bool,
}

#[derive(Serialize)]
struct Password {
    label: String,
    profile: Option<ProfileId>,
    password: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Wifi {
    ssid: String,
    broadcast_separately: bool,
    /// ISO 3166-1 alpha-2 regulatory country, written to every radio. `null`
    /// (the factory state) leaves the radios on the world domain.
    country: Option<String>,
    radios: HashMap<String, WifiRadio>,
    passwords: Vec<Password>,
}
// Response: Wifi
```

### `wifi.set`

```rust
// Request: Wifi fields (camelCase) flattened in, plus a confirm flag.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WifiSetRequest {
    #[serde(flatten)]
    wifi: Wifi<ProfileIdOpt>,               // wifi.get's response shape
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
// A `country` the regulatory database does not define is rejected with
// `InvalidValue` before anything is written; codes are upper-case, as in the
// database, so `us` is rejected. `null` clears it and returns the radios to the
// world domain.
// A radio whose channel or width changes, or that is being enabled, is rejected
// with `InvalidValue` unless `wifi.regulatory` lists that width for the channel
// (`autoWidths` for "auto"), or when the radio's PHY mode cannot express the
// width (HT stops at 40 MHz).
// A call that changes `country` instead rejects any width above 80 MHz.
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
#[serde(rename_all = "camelCase")]
struct BlackoutSetRequest {
    windows: Vec<BlackoutWindow>,
    /// Authorize turning Wi-Fi off now when a window covers the current time.
    #[serde(default)]
    confirm_immediate_blackout: bool,  // serialized as `confirmImmediateBlackout`
}

// Response:
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlackoutSetResult {
    /// True (and nothing applied) when a window covers the router's current
    /// time and the caller hasn't confirmed. False once applied.
    pending_immediate_blackout: bool,  // `pendingImmediateBlackout`
}
// Backend: without confirmation it validates, applies nothing, and reports
// whether the windows cover the current time in the router's timezone; with
// confirmation it saves them and applies the blackout state at once — Wi-Fi
// down inside a window, up outside every window (a radio disabled in its own
// settings stays down). Every Wi-Fi reload, boot, and timezone change applies
// that state again, and cron runs `wifi.blackout-apply` at each window boundary.
```

### `wifi.blackout-apply`

Internal endpoint, **not called from the frontend**. Applies the blackout state
for the current time, exactly as `wifi.blackout-set` does after saving. The
blackout crontab runs it at every window boundary.

```rust
// Request: {}
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

### `wifi.regulatory`

```rust
// Request: {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WifiRegulatory {
    /// ISO 3166-1 alpha-2 codes the firmware's regulatory database defines —
    /// the values `wifi.set` accepts for `country`.
    countries: Vec<String>,
    /// What an access point may use under the country currently in force,
    /// keyed by band ("2g", "5g"). Re-read after a `wifi.set` that changes
    /// `country`; a channel or width outside it leaves that radio down.
    bands: HashMap<String, BandChannels>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BandChannels {
    channels: Vec<ApChannel>,
    /// Widths automatic channel selection can serve without radar-detection
    /// channels.
    auto_widths: Vec<u32>,  // `autoWidths`
}

#[derive(Serialize)]
struct ApChannel {
    channel: u32,
    /// Widths in MHz (20, 40, 80, 160) whose every bonded 20 MHz channel is
    /// usable, using OpenWrt's bonding for the primary channel.
    widths: Vec<u32>,
}
// Response: WifiRegulatory
// Backend: reads `iw phy <phy> channels` for every radio. A channel is usable
// unless disabled or No IR; a width also needs the kernel's width flag on its
// channels.
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
enum LanAccess<Id> {
    #[serde(rename = "ALL")]
    All,
    #[serde(rename = "SAME_PROFILE")]
    SameProfile,
    /// Explicit list of allowed profiles — wire: `{ "other_profiles": [...] }`
    #[serde(rename = "other_profiles")]
    OtherProfiles(BTreeSet<Id>),
}

#[derive(Serialize, Deserialize)]
enum WanAccess {
    #[serde(rename = "ALL")]
    All,
    #[serde(rename = "NONE")]
    None,
    /// wire: `{ "whitelist": [...] }`
    #[serde(rename = "whitelist")]
    Whitelist(Vec<String>),
    /// wire: `{ "blacklist": [...] }`
    #[serde(rename = "blacklist")]
    Blacklist(Vec<String>),
}

/// `id: ProfileId` is flattened into the first three fields.
#[derive(Serialize)]
struct Profile<ProfileId> {
    fullname: String,
    interface: String,
    vlan_tag: u16,
    gateway_ip: String,
    /// "wan" for default WAN, or outbound VPN interface name
    outbound: String,
    lan_access: LanAccess<ProfileId>,
    wan_access: WanAccess,
    access_to_new_profiles: bool,
    owns_lan: bool,
    /// Always present in responses — an empty array when unset, never null
    #[serde(default)]
    dns_override: Vec<DnsServer>,
    /// "system", "custom", "vpn", or "system-outside-vpn" (a VPN-routed profile
    /// resolving through the ISP outside the tunnel)
    dns_source: String,
}
// Response: Profile<ProfileId>
```

`lan_access` returns the stored mode (`startwrt.<profile>.lan_access` and
`lan_access_list`); the profile-to-profile forwardings in `firewall` are derived
from it. `other_profiles` omits whitelisted profiles that no longer exist.

### `profiles.create`

```rust
/// `id: ProfileIdOpt` is flattened into the first three fields.
#[derive(Deserialize)]
struct Profile<ProfileIdOpt> {
    fullname: Option<String>,
    interface: Option<String>,
    vlan_tag: Option<u16>,
    gateway_ip: String,
    outbound: String,
    lan_access: LanAccess<ProfileIdOpt>,
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
//   outbound must be "wan" or an outbound VPN that vpn-client.list reports as
//   enabled: an unknown interface (or one that is a VPN server rather than a
//   client) is ErrorKind::NotFound, a disabled one ErrorKind::VpnDisabled.
//   A disabled VPN's tunnel never comes up, so routing a profile through it
//   would blackhole that profile.
```

On create and set, `lan_access: "ALL"` stores `access_to_new_profiles: true`,
`"SAME_PROFILE"` stores `false`, and `other_profiles` preserves the supplied flag.
An empty `other_profiles` with the flag false is stored as `"SAME_PROFILE"`.
Creating a profile adds it to every `other_profiles` list whose flag is true;
deleting one removes it from every list.

### `profiles.set`

```rust
// Request: the same flattened profile shape as profiles.create — all three id
// fields optional (they identify the profile to update, with profiles.get's
// lookup semantics) — plus a force flag.
#[derive(Deserialize)]
struct ProfileSetRequest {
    #[serde(flatten)]
    profile: Profile<ProfileIdOpt>,
    /// When true, forcibly delete VPN peers that would break due to the change.
    #[serde(default)]
    force: bool,
}
// Response: ProfileId
// Validation: same gateway_ip and outbound checks as profiles.create.
// Inbound VPN peers the new subnet would break fail the call with
//   ErrorKind::VpnPeersWouldBreak unless force is set.
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
struct SshKeyAddParams {
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
struct SshKeyDeleteParams {
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
// Backend: reads the SQLite activity log (/var/lib/startwrt/activity.db),
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
    /// POST the .tar.gz to /rest/rpc/{guid} (64 MiB body limit)
    upload: String,
}
// Backend: the upload continuation validates the archive (`tar -tzf`), applies
// it with `sysupgrade --restore-backup`, keeps the router's own system log
// preceded by any older history the backup carried, then reboots after a
// short delay
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
    filename: String,  // "diagnostics-startwrt-<YYYY-MM-DD>.log.gz"
}
// Backend: gzips a system summary (version, board, kernel, uptime, boot ID,
// eMMC wear), the persistent system log (`/var/log/messages.old` then
// `/var/log/messages`, or `logread` when neither exists) and the activity
// log, and registers a one-shot download continuation at /rest/rpc/{guid}
```

---

## 17. DNS Injection

### `dns.injected-list`

The DNS records permitted devices have published into the router's resolver
(see `devices.set-dns-injection` and `vpn-server.peer-set-dns-injection`).
Read-only: the device re-asserts or withdraws its own records, and the router
drops a record whose owner loses the address it published from.

```rust
// Request: {}

#[derive(Serialize)]
struct InjectedDnsRecord {
    name: String,
    /// "A", "AAAA", "CNAME", or "TXT".
    rtype: String,
    value: String,
    ttl: u32,
    /// The injecting device's address, when known.
    source: Option<String>,
    /// Owning LAN device MAC (uppercase); absent for a WireGuard peer.
    owner_mac: Option<String>,
    /// Owning inbound-VPN peer public key; absent for a LAN device.
    owner_peer: Option<String>,
    /// Display name of the owning LAN device, when one is known.
    device_name: Option<String>,
    /// Profile interface whose subnet the record was injected from.
    profile: Option<String>,
}
// Response: Vec<InjectedDnsRecord>
// Backend: the daemon's in-memory record store. Empty after a restart until
// devices re-assert.
```

---

## HTTP Routes

Every RPC method above is a JSON-RPC 2.0 call to a single endpoint: **`POST /rpc/v1`**.
The daemon (`backend/ctrl/src/bins/daemon.rs`) also serves:

| Route                                              | Method    | Auth                                       | Purpose                                                                                       |
| -------------------------------------------------- | --------- | ------------------------------------------ | --------------------------------------------------------------------------------------------- |
| `/rpc/v1`                                          | POST      | Request signature or local token (see § 1) | JSON-RPC 2.0 endpoint for all RPC methods                                                     |
| `/rest/rpc/{guid}`                                 | GET/POST  | GUID capability (one-shot)                 | RPC continuation: binary download (backup, diagnostics) / upload (restore); 64 MiB body limit |
| `/ws/rpc/{guid}`                                   | WebSocket | GUID capability (one-shot)                 | Streaming: update progress (`system.update`), live logs (`system.logs-follow`)                |
| `/api/setup/flash`                                 | POST      | None (setup wizard)                        | Streams NDJSON `SetupEvent` progress while flashing the eMMC; one flash at a time             |
| `/static/local-root-ca.crt`                        | GET       | None                                       | Root CA certificate download                                                                  |
| `/static/local-root-ca.mobileconfig`               | GET       | None                                       | Root CA as an Apple configuration profile                                                     |
| `/cgi-bin/*`, `/luci-static/*`, `/ubus`, `/ubus/*` | any       | LuCI's own                                 | Reverse proxy to uhttpd (LuCI) on localhost:8080; `/luci` redirects to `/cgi-bin/luci`        |
| everything else                                    | any       | None                                       | Embedded web UI                                                                               |

---

## Endpoint Summary

| RPC Method                          | Category        | Notes                       |
| ----------------------------------- | --------------- | --------------------------- |
| `auth.login`                        | Auth            | Rate-limited                |
| `auth.logout`                       | Auth            |                             |
| `auth.session.list`                 | Auth            |                             |
| `auth.session.kill`                 | Auth            |                             |
| `auth.verify-password`              | Auth            |                             |
| `auth.set-password`                 | Auth            |                             |
| `auth.check-initialized`            | Auth            | No auth                     |
| `auth.set-initial-password`         | Auth            | No auth                     |
| `system.info`                       | System          | No auth                     |
| `system.newer-versions`             | System          | No auth                     |
| `system.update`                     | System          |                             |
| `system.restart`                    | System          |                             |
| `system.factory-reset`              | System          |                             |
| `system.set-preferences`            | System          |                             |
| `system.apply-remote-access`        | System          | Internal, hotplug           |
| `system.set-timezone`               | System          |                             |
| `system.get-timezones`              | System          | No auth                     |
| `system.logs`                       | System          |                             |
| `system.logs-follow`                | System          |                             |
| `setup.status`                      | Setup           | No auth                     |
| `wan.ipv4-get`                      | WAN             |                             |
| `wan.ipv4-set`                      | WAN             |                             |
| `wan.ipv6-get`                      | WAN             |                             |
| `wan.ipv6-set`                      | WAN             |                             |
| `wan.mac-get`                       | WAN             |                             |
| `wan.mac-set`                       | WAN             |                             |
| `wan.dns-get`                       | WAN             |                             |
| `wan.dns-set`                       | WAN             |                             |
| `wan.ddns-get`                      | WAN             |                             |
| `wan.ddns-set`                      | WAN             |                             |
| `lan.ipv4-get`                      | LAN             |                             |
| `lan.ipv4-set`                      | LAN             |                             |
| `lan.ipv6-get`                      | LAN             |                             |
| `lan.ipv6-set`                      | LAN             |                             |
| `ethernet.get`                      | Ethernet        |                             |
| `ethernet.set`                      | Ethernet        |                             |
| `ethernet.edit`                     | Ethernet        | CLI editor                  |
| `devices.list`                      | Devices         |                             |
| `devices.update`                    | Devices         |                             |
| `devices.set-auto-forward`          | Devices         |                             |
| `devices.set-dns-injection`         | Devices         |                             |
| `devices.set-router-advertisements` | Devices         |                             |
| `devices.forget`                    | Devices         |                             |
| `devices.data-usage`                | Devices         |                             |
| `published-ports.list`              | Published Ports |                             |
| `published-ports.set`               | Published Ports |                             |
| `published-ports.auto-list`         | Published Ports | Automatic PCP/UPnP forwards |
| `published-ports.reconcile`         | Published Ports | Internal, hotplug           |
| `published-ports.wan-changed`       | Published Ports | Internal, hotplug           |
| `published-ports.sync-hairpin`      | Published Ports | Internal, WAN-schedule cron |
| `vpn-client.list`                   | Outbound VPN    |                             |
| `vpn-client.create`                 | Outbound VPN    |                             |
| `vpn-client.update`                 | Outbound VPN    |                             |
| `vpn-client.delete`                 | Outbound VPN    |                             |
| `vpn-client.set-enabled`            | Outbound VPN    |                             |
| `vpn-server.list`                   | Inbound VPN     |                             |
| `vpn-server.set`                    | Inbound VPN     |                             |
| `vpn-server.delete`                 | Inbound VPN     |                             |
| `vpn-server.peer-add`               | Inbound VPN     |                             |
| `vpn-server.peer-delete`            | Inbound VPN     |                             |
| `vpn-server.peer-rename`            | Inbound VPN     |                             |
| `vpn-server.peer-set-dns-injection` | Inbound VPN     |                             |
| `wifi.get`                          | WiFi            |                             |
| `wifi.set`                          | WiFi            |                             |
| `wifi.edit`                         | WiFi            | CLI editor                  |
| `wifi.blackout-get`                 | WiFi            |                             |
| `wifi.blackout-set`                 | WiFi            |                             |
| `wifi.blackout-apply`               | WiFi            | Internal, blackout cron     |
| `wifi.generate-password`            | WiFi            |                             |
| `wifi.regulatory`                   | WiFi            |                             |
| `profiles.list`                     | Profiles        |                             |
| `profiles.get`                      | Profiles        |                             |
| `profiles.create`                   | Profiles        |                             |
| `profiles.set`                      | Profiles        |                             |
| `profiles.delete`                   | Profiles        |                             |
| `profiles.edit`                     | Profiles        | CLI editor                  |
| `profiles.schedule-get`             | Profiles        |                             |
| `profiles.schedule-set`             | Profiles        |                             |
| `ssh-keys.list`                     | SSH Keys        |                             |
| `ssh-keys.add`                      | SSH Keys        |                             |
| `ssh-keys.delete`                   | SSH Keys        |                             |
| `activity.list`                     | Activity        |                             |
| `activity.delete`                   | Activity        |                             |
| `activity.clear`                    | Activity        |                             |
| `backup.create`                     | Backup          |                             |
| `backup.restore`                    | Backup          |                             |
| `diagnostics.create`                | Diagnostics     |                             |
| `dns.injected-list`                 | DNS Injection   |                             |

**Totals:** 84 RPC methods across 17 categories, plus the HTTP/WebSocket routes
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
