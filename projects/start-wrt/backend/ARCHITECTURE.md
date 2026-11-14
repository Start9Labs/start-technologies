# Backend Architecture

Rust crates powering the StartWRT router daemon and CLI. Three crates: `ctrl` (RPC server + CLI), `uciedit` (UCI parser library), `uciedit_macros` (proc macros for typed UCI sections).

## Transport

HTTP server on ports 80 (HTTP) and 443 (HTTPS, if TLS setup succeeds). JSON-RPC 2.0 requires signed-DH admission at `/rpc/v2`, with `POST /rpc/v1` compatibility, via [rpc-toolkit](../../../shared-libs/crates/rpc-toolkit). `rpc.rs` composes one dispatch/middleware stack, adds typed firmware metadata to replies and retains the CORS-exposed HTTP firmware header. Live authorization uses the verified connection peer; request-signature/local-token compatibility stays in `start-core` (see [auth.rs](#authrs--authentication)).

Additional routes:

| Route                                     | Purpose                                                                                                 |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `POST /api/setup/flash`                   | Legacy NDJSON flash observation                                                                         |
| `GET\|POST /rest/rpc/{guid}`              | Continuation endpoint for backup/restore/diagnostics (64 MiB limit)                                     |
| `GET /rpc/v2/{guid}`                      | WebSocket continuation endpoint (update progress, live logs); `/ws/rpc/{guid}` is a compatibility alias |
| `GET /luci`                               | Convenience redirect to `/cgi-bin/luci`                                                                 |
| `GET /static/local-root-ca.crt`           | Root CA certificate download (no auth)                                                                  |
| `GET /static/local-root-ca.mobileconfig`  | Root CA as an Apple configuration profile (no auth)                                                     |
| `/cgi-bin/*`, `/luci-static/*`, `/ubus/*` | LuCI reverse proxy (localhost:8080), HTTPS only                                                         |
| Fallback                                  | Serves embedded web UI via `include_dir`                                                                |

Beside the wildcard listeners, port control binds the SNI demux to the WAN IPv4 on 443 while hostname routes hold it (SO_REUSEPORT; the kernel delivers a connection to the most specific bound socket).

Port 80 is decided per request (`http_redirect.rs`). An outermost layer serves the UI to a client on a connected subnet off the WAN, at an address that is not the WAN's, and answers every other IPv4 request with a 307 to the same authority over HTTPS. While 443 is published and the WAN address is known, port control keeps a WAN ACCEPT rule on tcp/80 at that address in the SNI admission set, rewritten when the address changes. The layer checks the client's subnet as well as the address it dialed. Its `SNI` label makes a build without the redirect purge it; its `name` tells it from a hostname route's rule on 80, which takes precedence. The gate opens before the rule is written, shuts after the reload that removes it, and is seeded from any `SNI` rule on 80 before port control starts. The router's addresses are read at seeding, on every sweep and WAN hotplug, and when a request would redirect on stale addresses; unreadable, they leave no client trusted.

## Server vs CLI

Both `startwrt-ctrld` (daemon) and `startwrt-cli` (CLI) share the same handler code via the `CtrlContext` trait:

```rust
pub trait CtrlContext: Context + Clone {
    fn uci_root(&self) -> PathBuf;   // /etc/config/ on router, configurable for CLI
    fn effectful(&self) -> bool;      // CLI: --configs-only skips service reloads
    fn wan_ipv4_addrs(&self) -> Vec<Ipv4Addr>; // provided; empty unless effectful
}
```

- **`ServerContext`**: always reads `/etc/config/`, always reloads services. Holds `start-core`'s `RpcContinuations` for long-running operations and the auth state (enrolled keys, nonce cache). `ServerContext::default()` carries an in-memory auth state that enrolls nothing; only the daemon opens `/etc/startwrt/auth.db`. The daemon loads its separate server identity from `serverIdentityKey` in `/etc/startwrt/auth.db` through `start-core`'s instance-key loader; sysupgrade keeps that database. In-memory contexts generate an ephemeral server identity. The daemon-local setup flash gate is shared by typed RPC and legacy NDJSON. Its owned mutex guard stays with accepted work on start-core's `make_send` runtime; observer cancellation cannot release it or abort provisioning. Actual worker results supply typed terminals or legacy Complete/Error events. Shared blocking work participates in the daemon's existing 60-second runtime shutdown grace; forced termination or reboot can still interrupt writes. Optional immutable `SecureSession` provenance belongs to each context clone rather than the shared auth state; WebSocket continuation creators capture it independently of revocation and resource policy.
- **`CliContext`**: configurable root (`--config-root`), calls the daemon when needed. The identity at `~/.startwrt/id.key.pem` signs a fresh ephemeral-DH hello through the shared opaque signer before transport deadlines. Local tokens take precedence, restricted to loopback hosts; token and missing-key calls use the existing SessionClient's unpersisted proof identity without enrollment. Malformed keys fail closed. The ordinary HTTP client retains its network policy; an HTTP/1-only, nonredirecting upgrade client uses the shared encrypted transport. Login retry and command output stay in the CLI owner. Log/update commands return their existing GUID values rather than consuming the streams; browser continuation owners redeem secure-origin GUIDs through fresh same-peer exchanges.

The single binary `startwrt` uses `MultiExecutable` to dispatch based on the symlink name (`startwrt-ctrld` or `startwrt-cli`) or the first argument.

### CLI Arguments

```
--config-root PATH   UCI config directory (default: /etc/config)
--configs-only       Skip service reloads (write configs only)
--host URL           Server URL (default: http://127.0.0.1/rpc/v1 on the router, else https://router.lan/rpc/v1)
--root-ca PEM_PATH   Certificate authority to trust, such as the router's from /static/local-root-ca.crt (repeatable)
```

### Local-Only Subcommands

These run directly without connecting to the server:

- `startwrt set-wifi-password [--manual]` — Provision WiFi password into UCI when EEPROM tag 0x2F is missing
- `startwrt flash` — Interactive firmware flash from microSD to eMMC
- `startwrt verify` — Factory QC checks: firmware integrity, EEPROM WiFi password, SSID broadcast

## RPC Method Tree

All methods assembled in `main_api()` → `ParentHandler<C>`:

```
auth
  ├─ login              # Enroll the signing key (rate-limited: 3 per 20s)
  ├─ logout             # Revoke the signing key
  ├─ session list / kill
  ├─ verify-password    # Password check without login
  ├─ set-password        # Change password
  ├─ check-initialized  # Has admin password been set?
  └─ set-initial-password

profiles
  ├─ list / get / create / set / delete / edit
  ├─ schedule-get       # WAN access schedules per profile
  └─ schedule-set

ethernet
  ├─ get                # Port-to-profile mapping
  ├─ set
  └─ edit               # Partial update

wifi
  ├─ get / set / edit   # SSID, passwords, radio settings
  ├─ blackout-get / blackout-set / blackout-apply
  ├─ generate-password
  └─ regulatory        # Countries + per-band channels

vpn-server              # Inbound WireGuard VPN
  ├─ list / set / delete
  └─ peer-add / peer-delete / peer-rename / peer-set-dns-injection

vpn-client              # Outbound WireGuard VPN
  ├─ list / create / update / delete
  └─ set-enabled

setup
  ├─ status             # Setup mode detection, disk state
  ├─ flash              # Typed progress and actual provisioning terminal
  └─ get-pubkey         # Ephemeral key the setup password is encrypted to

system                  # System settings, remote access, restart, factory reset,
                        # OTA update (newer-versions / update)

devices                 # Device enumeration, rename, forget, data usage

dns
  └─ injected-list      # Records devices published via DNS UPDATE

wan                     # WAN IPv4/IPv6, MAC, DNS, DDNS config

lan                     # LAN IPv4/IPv6 config

published-ports         # Port forwarding rules

ssh-keys                # SSH public key management

activity                # Activity log (list, delete, clear)

backup                  # Backup create/restore (via continuations)

diagnostics             # Diagnostic bundle creation

config
  └─ check              # Configuration a page cannot represent, with its pages

uci                     # Generic UCI get/set/edit (legacy)
  ├─ get
  ├─ set
  └─ edit

file                    # Generic file read/write (legacy)
  ├─ get / set
dir
  └─ get

exec                    # Shell command execution
```

## Modules

### profiles.rs — Security Profile CRUD

The core module. Each profile creates a VLAN on the LAN bridge, a network interface, a firewall zone with forwarding rules, and a DHCP server. Orchestrates changes across five UCI configs: `startwrt`, `network`, `firewall`, `dhcp`, `wireless`. Uses retry loops (4 retries) for conflict resolution.

Key types:

- `Profile<Id>` — gateway IP, outbound route, `LanAccess` (All/SameProfile/OtherProfiles), `WanAccess` (All/None/Whitelist/Blacklist)
- `ProfileId` — fullname + interface name + VLAN tag
- `Lookup` — index for resolving profiles across configs

Also handles WAN access schedules — time-of-day rules that enable/disable WAN access per profile via firewall rules.

### wifi.rs — WiFi Configuration

Manages SSID, per-radio settings (band, channel, enabled, broadcast), and multi-password config. Each password maps to a profile via dynamic VLAN. `dynamic_vlan` is set per `wifi-iface`: `WifiDynamicVlan::ALLOWED`, with the first profile-less (admin) password as the interface `key`, when one exists; otherwise `REQUIRED` with no key.

Manages WiFi blackout schedules: every Wi-Fi reload re-applies the state for the current time, and crontab (`/etc/crontabs/root`) lines re-apply it at each window boundary.

Creates `WifiStation` (password entries) and `WifiVlan` (VLAN mapping) sections in the `wireless` UCI config.

### ethernet.rs — Ethernet Port Assignment

Maps physical ports to profiles via bridge VLAN assignments on the LAN bridge (`br-lan`). Also manages WAN port designation (which port is the uplink). Reads/writes `NetworkBridgeVlan` and `NetworkVlanPort` in the `network` UCI config. USB ports reach the bridge through `usb_ports.rs`, and a request that omits a recorded USB port keeps its profile.

### vpn_server.rs / vpn_client.rs — VPN Management

- **vpn_server**: Inbound WireGuard servers. Each server interface bound to a profile. Manages peer key generation, allowed IPs, and warns before deleting servers that would break peer connectivity.
- **vpn_client**: Outbound WireGuard clients. Supports VPN chaining (client A routes through client B). Detects cycles and warns before deleting clients that are in use by profiles or other VPN clients.

### auth.rs — Authentication

The daemon runs `start-core`'s `Auth` middleware with `LocalAuth` and `SignatureAuth`; `auth.rs` supplies the StartWRT side of their context traits.

- The handler tree is `start-core`'s `auth()` (login, logout, session list/kill) with StartWRT's password handlers chained on, the same shape as StartTunnel's.
- Enrolled keys live in a patch-db at `/etc/startwrt/auth.db`, listed in sysupgrade's `keep.d`. A key's `lastActive` is written at most hourly (`LAST_ACTIVE_RESOLUTION`), so routine requests never write to flash. `lastActive` is shown by `auth session list` and governs nothing. `check_pubkey` refuses a key 24 hours after its `loggedIn` time (`SIGN_IN_LIFETIME`); until `/var/state/dnsmasqsec` marks the clock trusted it judges age alone, and afterwards a `loggedIn` more than `LOGIN_CLOCK_SKEW` ahead also expires. `start-core`'s `reap_idle_keys`, run hourly once the clock is trusted, unenrolls expired keys through `LoginContext::sign_in_expired`.
- Signing identities: `router.lan`, `router.local`, every address on the router's interfaces (loopback and link-local IPv6 excluded; start-core accepts a loopback address from a loopback peer), and the enabled DDNS hostname, re-read only when `/etc/config/ddns` changes.
- Timestamps are enforced once `/var/state/dnsmasqsec` exists (dnsmasq's ntp hotplug writes it when ntpd reports valid time); before that, only the nonce cache stops replays.
- Password validated against `/etc/shadow` (or `STARTWRT_DEV_PASSWORD` env var in dev)
- Rate limiting: 3 login attempts per 20 seconds per signed identity
- Local auth token: generated at daemon startup → `/run/startwrt/rpc.authcookie` (`root:root`, `0640`)

Handler metadata:

- `authenticated: false` — no credential needed (status endpoints, first-time setup)
- `login: true` — enroll the signing key by password; reject enrollment when a valid local auth token is supplied
- `get_signer: true` — inject the signing key as `__Auth_signer`

Every other call needs an enrolled encrypted peer, a valid compatibility signature or the local token; the peer address grants nothing. A secure-origin GUID requires fresh same-peer encrypted admission signed for that GUID, independently of live revocation and resource budgets.

Returns RPC error code 34 on auth failure (frontend auto-logs out).

### uci.rs, files.rs, exec.rs — Generic/Legacy

Low-level UCI, file, and shell access. These are vestigial — all features now have purpose-built smart endpoints, and no frontend code calls these generic methods. They remain in the API surface but have no callers.

### Other Modules

| Module                            | Purpose                                                                                                                                                                                                                                                      |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `system.rs`                       | System settings, timezone, remote access rules, logs, OTA entry points, restart, factory reset                                                                                                                                                               |
| `devices.rs`                      | Device enumeration from ARP/DHCP, rename, forget (flush ARP + lease)                                                                                                                                                                                         |
| `wan.rs` / `lan.rs`               | WAN/LAN interface configuration                                                                                                                                                                                                                              |
| `usb_ports.rs`                    | USB ports as LAN ports: records each physical USB port an adapter is plugged into, bridges the adapter into `br-usb_eth<N>`, joined to `br-lan` by the `usb_eth<N>` veth, and removes an empty port on request                                               |
| `published_ports.rs`              | Port forwarding rules (firewall redirects)                                                                                                                                                                                                                   |
| `port_control.rs`                 | Automatic port forwarding: PCP + UPnP IGD servers on the shared `start-core` protocol cores, mapping device-requested forwards onto auto-tagged UCI redirects with in-memory lease expiry; also owns the SNI demux and the port-80 redirect's admission rule |
| `http_redirect.rs`                | HTTP→HTTPS redirect on port 80 while 443 is published: a request layer keyed on the client's subnet and the dialed address                                                                                                                                   |
| `ssh_keys.rs`                     | SSH public key CRUD (`/etc/dropbear/authorized_keys`)                                                                                                                                                                                                        |
| `vpn_client.rs` / `vpn_server.rs` | WireGuard VPN management                                                                                                                                                                                                                                     |
| `dns.rs`                          | DNS server configuration                                                                                                                                                                                                                                     |
| `dns_inject.rs`                   | RFC 2136 DNS UPDATE ingress: records published by permitted devices and inbound VPN peers, served through per-profile dnsmasq (`dns.injected-list`)                                                                                                          |
| `activity.rs`                     | Activity logging via SQLite                                                                                                                                                                                                                                  |
| `backup.rs` / `diagnostics.rs`    | Backup/restore and diagnostic bundles (via continuations)                                                                                                                                                                                                    |
| `config_check.rs`                 | Finds configuration, typically from LuCI or a hand edit, that a page cannot represent (`config.check`)                                                                                                                                                       |
| `logs.rs`                         | Log snapshot and `system.logs-follow` streaming                                                                                                                                                                                                              |
| `ssl.rs`                          | Root CA and intermediate CA, their files under `/etc/ssl`, and the Root CA download                                                                                                                                                                          |
| `tls.rs`                          | HTTPS certificates: a local-CA leaf per name served (`start-core` `CertStore`)                                                                                                                                                                               |
| `init.rs`                         | Early boot WiFi config, password generation                                                                                                                                                                                                                  |
| `setup.rs` / `flash.rs`           | Setup wizard, firmware flashing                                                                                                                                                                                                                              |
| `captive.rs`                      | Captive portal management                                                                                                                                                                                                                                    |
| `embedded_web.rs`                 | Serves the Angular SPA from `include_dir`                                                                                                                                                                                                                    |
| `luci_proxy.rs`                   | Reverse proxy to LuCI on localhost:8080                                                                                                                                                                                                                      |
| `continuations.rs`                | Serves `start-core` RPC continuations at `/rest/rpc/{guid}`                                                                                                                                                                                                  |
| `update.rs`                       | OTA update engine — download/verify/flash, progress via continuations                                                                                                                                                                                        |
| `registry/`                       | Registry client (`device_info`, `asset`, `os`, `signer`)                                                                                                                                                                                                     |
| `sign/`                           | ed25519 signature + commitment verification                                                                                                                                                                                                                  |
| `progress.rs`                     | Progress reporting, wire-compatible with start-os                                                                                                                                                                                                            |
| `eeprom.rs`                       | EEPROM TLV read (ONIE blob; WiFi PMK in tag 0x2F)                                                                                                                                                                                                            |
| `wg.rs`                           | WireGuard key management types                                                                                                                                                                                                                               |
| `device_names.rs`                 | Persistent cache of auto-learned device hostnames                                                                                                                                                                                                            |
| `verify.rs`                       | Factory QC checks (firmware integrity, EEPROM password, SSID broadcast)                                                                                                                                                                                      |
| `templates/`                      | `client.conf.template` — WireGuard peer client-config template                                                                                                                                                                                               |
| `error.rs`                        | `ErrorKind` enum and `Error` type                                                                                                                                                                                                                            |

## TypeScript bindings

RPC structs derive `visit_rs::VisitFields` and enums derive
`visit_rs::VisitVariants`, supplying raw facts and static traversal.
Adjacent `rpc_toolkit::reflect_ts!` bridges normalize serde metadata and render TypeScript while retaining generic
payload bounds and default generic inlining. RPC interprets raw `visit(ts(...))`
hints; signing containers use explicit storage opacity and a `string` literal.
Local `type_attributes` selectors collect compiler-resolved wire/conversion types
from existing literals. Raw descriptors retain every member; serde skips and
renames do not affect reflection. Streaming roots register beside their owners
with `rpc_toolkit::ts_export!`;
`ctrl/examples/generate_rpc_bindings.rs` collects the `events` namespace and
traverses `main_api()` for the RPC tree.

## UCI Library (uciedit)

### Parsing

Arena-based zero-copy parser. `Config::parse()` reads a UCI file into a tree of `Section`s, each containing `Line`s (package, option, list, comment, empty). Preserves all formatting and comments. It accepts libuci's syntax and decodes each value the way libuci's `parse_str` does; new values are written in libuci's quoting (`'…'`, with `'` as `'\''`).

```rust
let arena = Arena::new();
let config = Config::parse(&arena, Path::new("/etc/config/network")).await?;
let cfgs = parse_all(Path::new("/etc/config"), &arena, &["network", "firewall"]).await?;
```

### Writing

Atomic writes via temp-file + rename. Conflict detection compares file mtime at parse vs write time — if the file changed since parsing, the write fails with `Error::Conflict`.

```rust
config.dump(Path::new("/etc/config/network")).await?;   // Single config
dump_all(Path::new("/etc/config"), cfgs).await?;        // Multiple configs
```

### Typed Sections

`#[derive(TypedSection)]` provides compile-time-safe access to UCI config sections:

```rust
#[derive(TypedSection)]
#[uci(ty = "zone")]
struct FirewallZone {
    name: String,
    input: String,
    output: String,
    forward: String,
    #[uci(default)]
    network: Vec<String>,
}
```

Macro attributes:

- `#[uci(ty = "zone")]` — UCI section type name
- `#[uci(rename = "type")]` — field name differs from UCI option name
- `#[uci(default)]` — use `Default::default()` if option missing
- `#[uci(default_value = expr)]` — custom default value
- `#[uci(inpt)]` — use `inpt` parser instead of `FromStr`
- `#[uci(accept_list)]` — a single-valued field also reads `list` lines of that name, as libuci does: its value is the last `option`, else the first item. A write replaces that value, keeps the items after it, and drops the lines it overrides
- A `Vec` field also reads an `option` line of that name as its whitespace-separated words, the way OpenWrt's own scripts do

Shared, OpenWrt-native typed sections live in `uciedit/src/openwrt.rs`; feature-owned sections (`UciPreferences`, `UciWifiBlackout`, `UciProfile`, the VPN sections, …) are defined in `ctrl` beside their handlers. The sections in `openwrt.rs`:

| Config       | Structs                                                                                                                                    |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| **firewall** | `FirewallZone`, `FirewallRule`, `FirewallRedirect`, `FirewallForwarding`                                                                   |
| **network**  | `NetworkInterface`, `NetworkDevice`, `NetworkBridgeVlan`, `NetworkRoute`, `NetworkRoute6`, `NetworkRule`, `NetworkRule6`, `NetworkGlobals` |
| **wireless** | `WifiDevice`, `WifiInterface`, `WifiVlan`, `WifiStation`                                                                                   |
| **dhcp**     | `Dhcp`, `DhcpHost`, `ProfileDnsmasq`                                                                                                       |
| **startwrt** | `UciSystemDns` (the `system_dns` section)                                                                                                  |
| **ddns**     | `DdnsService`                                                                                                                              |

`NetworkVlanPort` (a plain struct) and `NetworkVlanPortTagging` (an enum) are not `TypedSection`s — they are used as fields within `NetworkBridgeVlan`.

## Error Types

`ErrorKind` enum covers IO, UCI parse/write errors, and domain errors:

- Config errors: `InterfaceNameConflict`, `DuplicateVlanTag`, `DuplicatePassword`, `DuplicatePasswordLabel`, `DuplicateFullname`
- Profile errors: `MissingProfile`, `CorruptedProfile`, `LanOwnerExists`, `CannotDeleteLanOwner`
- Infrastructure errors: `MissingLanBridge`, `MissingWanInterface`, `MissingLanInterface`, `MissingFirewallZone`
- WiFi errors: `CorruptedWifi`, `UnnamedWirelessInterface`, `UnnamedWirelessDevice`
- VPN errors: `VpnHasDependents`, `VpnChainCycle`, `VpnPeersWouldBreak`, `VpnDisabled`
- Validation: `InvalidValue`, `WanPortWithProfile`, `DhcpStaticHostsInSubnet`, `SubnetCollision`, `MissingDeviceAddress`, `PublishedPortsUseIpv6`
- UCI errors: `UciEdit` (any `uciedit` parse, I/O, or write-conflict failure, and failed `uci` CLI calls)

An RPC error carries its `ErrorKind` value as `code` and the kind's name as `message`. Code 34 (authorization) triggers frontend logout.

## Common Pitfalls

- **UCI conflicts.** Concurrent writes to the same config file will fail with `uciedit::Error::Conflict`. Re-read, re-apply, and re-write in a bounded loop; profile creation retries 4 times.
- **Service reloads.** After writing UCI configs, call `/etc/init.d/<service> reload`. The `effectful` flag on `CtrlContext` controls whether reloads happen.
- **Arena lifetimes.** uciedit uses arena allocation — parsed values borrow from the arena. Don't try to outlive it.
- **Bridge VLAN filtering.** StartWRT uses UCI `bridge-vlan` configuration for per-port VLAN assignment. OpenWrt maps this to DSA hardware switch tables when available (e.g., DeepComputing 4-port board) or to the kernel's software bridge filter when not (e.g., BPI-F3, which has two independent GMACs and no switch chip). The backend code is the same for both — it writes `NetworkBridgeVlan` / `NetworkVlanPort` sections regardless of the underlying hardware.
