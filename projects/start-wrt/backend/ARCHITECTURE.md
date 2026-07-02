# Backend Architecture

Rust workspace powering the StartWRT router daemon and CLI. Three crates: `ctrl` (RPC server + CLI), `uciedit` (UCI parser library), `uciedit_macros` (proc macros for typed UCI sections).

## Transport

HTTP server on ports 80 (HTTP) and 443 (HTTPS, if TLS setup succeeds). JSON-RPC 2.0 at `POST /rpc/v1` via [rpc-toolkit](https://github.com/Start9Labs/rpc-toolkit). Session cookie authentication with rate limiting.

Additional routes:

| Route | Purpose |
|-------|---------|
| `GET /api/logs` | WebSocket for live log streaming |
| `POST /api/setup/flash` | NDJSON streaming for setup wizard |
| `GET\|POST /rest/rpc/{guid}` | Continuation endpoint for backup/restore/diagnostics (10MB limit) |
| `GET /static/root-ca.crt` | Root CA certificate download (no auth) |
| `/cgi-bin/*`, `/luci-static/*`, `/ubus/*` | LuCI reverse proxy (localhost:8080) |
| Fallback | Serves embedded web UI via `include_dir` |

## Server vs CLI

Both `startwrt-ctrld` (daemon) and `startwrt-cli` (CLI) share the same handler code via the `CtrlContext` trait:

```rust
pub trait CtrlContext: Context + Clone {
    fn uci_root(&self) -> PathBuf;   // /etc/config/ on router, configurable for CLI
    fn effectful(&self) -> bool;      // CLI: --configs-only skips service reloads
}
```

- **`ServerContext`**: always reads `/etc/config/`, always reloads services. Holds `RpcContinuations` for long-running operations.
- **`CliContext`**: configurable root (`--config-root`), cookie persistence in `~/.startwrt/.cookies.json`, calls server via HTTP when needed. Injects local auth cookie from `/run/startwrt/rpc.authcookie` when running on the router.

The single binary `startwrt` uses `MultiExecutable` to dispatch based on the symlink name (`startwrt-ctrld` or `startwrt-cli`) or the first argument.

### CLI Arguments

```
--config-root PATH   UCI config directory (default: /etc/config)
--configs-only       Skip service reloads (write configs only)
--host URL           Server URL (default: http://router.lan/rpc/v1)
```

### Local-Only Subcommands

These run directly without connecting to the server:

- `startwrt set-wifi-password [--manual]` — Provision WiFi password into UCI when EEPROM tag 0x2F is missing
- `startwrt flash` — Interactive firmware flash from microSD to eMMC
- `startwrt verify` — Verification

## RPC Method Tree

All methods assembled in `main_api()` → `ParentHandler<C>`:

```
auth
  ├─ login              # Session creation (rate-limited: 3 per 20s)
  ├─ logout             # Session destruction
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
  ├─ get / set          # SSID, passwords, radio settings
  ├─ blackout-get / blackout-set
  └─ generate-password

vpn-server              # Inbound WireGuard VPN
  ├─ list / set / delete
  └─ peer-add / peer-delete

vpn-client              # Outbound WireGuard VPN
  ├─ list / create / update / delete
  └─ set-enabled

setup
  └─ status             # Setup mode detection, disk state

system                  # System settings, remote access, restart, factory reset

devices                 # Device enumeration, rename, forget, data usage

wan                     # WAN IPv4/IPv6, MAC, DNS, DDNS config

lan                     # LAN IPv4/IPv6 config

published-ports         # Port forwarding rules

ssh-keys                # SSH public key management

activity                # Activity log (list, delete, clear)

backup                  # Backup create/restore (via continuations)

diagnostics             # Diagnostic bundle creation

uci                     # Generic UCI get/set (legacy, being replaced)
  ├─ get
  └─ set

file                    # Generic file read/write (legacy)
  ├─ get / set
dir
  └─ get

exec                    # Shell command execution (being phased out)
```

## Modules

### profiles.rs — Security Profile CRUD

The core module. Each profile creates a VLAN on the LAN bridge, a network interface, a firewall zone with forwarding rules, and a DHCP server. Orchestrates changes across four UCI configs: `startwrt`, `network`, `firewall`, `dhcp`. Uses retry loops (4 attempts) for conflict resolution.

Key types:
- `Profile<Id>` — gateway IP, outbound route, `LanAccess` (All/SameProfile/OtherProfiles), `WanAccess` (All/None/Whitelist/Blacklist)
- `ProfileId` — fullname + interface name + VLAN tag
- `Lookup` — index for resolving profiles across configs

Also handles WAN access schedules — time-of-day rules that enable/disable WAN access per profile via firewall rules.

### wifi.rs — WiFi Configuration

Manages SSID, per-radio settings (band, channel, enabled, broadcast), and multi-password config. Each password maps to a profile via dynamic VLAN. The admin password uses `WifiDynamicVlan::ALLOWED`; all others use `REQUIRED`.

Manages WiFi blackout schedules via crontab (`/etc/crontabs/root`).

Creates `WifiStation` (password entries) and `WifiVlan` (VLAN mapping) sections in the `wireless` UCI config.

### ethernet.rs — Ethernet Port Assignment

Maps physical ports to profiles via bridge VLAN assignments on the LAN bridge (`br-lan`). Also manages WAN port designation (which port is the uplink). Reads/writes `NetworkBridgeVlan` and `NetworkVlanPort` in the `network` UCI config.

### vpn_server.rs / vpn_client.rs — VPN Management

- **vpn_server**: Inbound WireGuard servers. Each server interface bound to a profile. Manages peer key generation, allowed IPs, and warns before deleting servers that would break peer connectivity.
- **vpn_client**: Outbound WireGuard clients. Supports VPN chaining (client A routes through client B). Detects cycles and warns before deleting clients that are in use by profiles or other VPN clients.

### auth.rs — Session Authentication

- Sessions stored as JSON in `/etc/startwrt/sessions.json`
- Password validated against `/etc/shadow` (or `STARTWRT_DEV_PASSWORD` env var in dev)
- Tokens: random bytes → base32 encoding (sent to client), SHA-256 hash (stored server-side)
- 1-day session expiry, HTTP-only SameSite=Strict cookie
- Rate limiting: 3 login attempts per 20 seconds
- Local auth cookie: generated at daemon startup → `/run/startwrt/rpc.authcookie`, read by CLI to bypass session auth over SSH

### middleware/auth.rs — HTTP Session Middleware

`SessionAuth` middleware applied to all RPC endpoints. Uses handler metadata to determine behavior:

- `login: true` — rate-limit, inject user agent
- `no_auth: true` — skip auth (status endpoints)
- `get_session: true` — inject `sessionHash` into params

Returns RPC error code 34 on auth failure (frontend auto-logs out).

### uci.rs, files.rs, exec.rs — Generic/Legacy

Low-level UCI, file, and shell access. These are vestigial — all features now have purpose-built smart endpoints, and no frontend code calls these generic methods. They remain in the API surface but have no callers.

### Other Modules

| Module | Purpose |
|--------|---------|
| `system.rs` | System settings, remote access rules, schedules, restart, factory reset |
| `devices.rs` | Device enumeration from ARP/DHCP, rename, forget (flush ARP + lease) |
| `wan.rs` / `lan.rs` | WAN/LAN interface configuration |
| `published_ports.rs` | Port forwarding rules (firewall redirects) |
| `ssh_keys.rs` | SSH public key CRUD (`/etc/dropbear/authorized_keys`) |
| `vpn_client.rs` / `vpn_server.rs` | WireGuard VPN management |
| `dns.rs` | DNS server configuration |
| `activity.rs` | Activity logging via SQLite |
| `backup.rs` / `diagnostics.rs` | Backup/restore and diagnostic bundles (via continuations) |
| `logs.rs` | WebSocket log streaming |
| `ssl.rs` | TLS cert generation: Root CA → Intermediate CA → Server cert |
| `init.rs` | Early boot WiFi config, password generation |
| `setup.rs` / `flash.rs` | Setup wizard, firmware flashing |
| `captive.rs` | Captive portal management |
| `embedded_web.rs` | Serves the Angular SPA from `include_dir` |
| `luci_proxy.rs` | Reverse proxy to LuCI on localhost:8080 |
| `continuations.rs` | Long-running operation handling (GUID-based, timeout, kill signals) |
| `error.rs` | `ErrorKind` enum and `Error` type |

## UCI Library (uciedit)

### Parsing

Arena-based zero-copy parser. `Config::parse()` reads a UCI file into a tree of `Section`s, each containing `Line`s (option, list, comment, empty). Preserves all formatting and comments.

```rust
let arena = Arena::new();
let config = Config::parse("network", Path::new("/etc/config"), &arena)?;
let cfgs = parse_all(Path::new("/etc/config"), &arena, &["network", "firewall"])?;
```

### Writing

Atomic writes via temp-file + rename. Conflict detection compares file mtime at parse vs write time — if the file changed since parsing, the write fails with `Error::Conflict`.

```rust
config.dump()?;         // Single config
dump_all(&mut cfgs)?;   // Multiple configs
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

All typed sections live in `uciedit/src/openwrt.rs`:

| Config | Structs |
|--------|---------|
| **firewall** | `FirewallZone`, `FirewallRule`, `FirewallRedirect`, `FirewallForwarding` |
| **network** | `NetworkInterface`, `NetworkDevice`, `NetworkBridgeVlan`, `NetworkRoute`, `NetworkRule` |
| **wireless** | `WifiDevice`, `WifiInterface`, `WifiVlan`, `WifiStation` |
| **dhcp** | `Dhcp`, `DhcpHost`, `ProfileDnsmasq` |
| **system** | `UciSystemDns`, `DdnsService` |

`NetworkVlanPort` and `NetworkVlanPortTagging` are plain structs (not `TypedSection`) used as fields within `NetworkBridgeVlan`.

## Error Types

`ErrorKind` enum covers IO, UCI parse/write errors, and domain errors:

- Config errors: `InterfaceNameConflict`, `DuplicateVlanTag`, `DuplicatePassword`, `DuplicatePasswordLabel`, `DuplicateFullname`
- Profile errors: `MissingProfile`, `CorruptedProfile`, `LanOwnerExists`, `CannotDeleteLanOwner`
- Infrastructure errors: `MissingLanBridge`, `MissingWanInterface`, `MissingLanInterface`, `MissingFirewallZone`
- WiFi errors: `CorruptedWifi`, `UnnamedWirelessInterface`, `UnnamedWirelessDevice`
- VPN errors: `VpnHasDependents`, `VpnChainCycle`, `VpnPeersWouldBreak`, `MissingDeviceAddress`
- Validation: `InvalidValue`, `WanPortWithProfile`

Auth-specific RPC error codes: 7 (incorrect password), 34 (authorization — triggers frontend logout), 40 (uninitialized).

## Common Pitfalls

- **UCI conflicts.** Concurrent writes to the same config file will fail. Use uciedit's retry mechanism. Profile creation already retries 4 times.
- **Service reloads.** After writing UCI configs, call `/etc/init.d/<service> reload`. The `effectful` flag on `CtrlContext` controls whether reloads happen.
- **Arena lifetimes.** uciedit uses arena allocation — parsed values borrow from the arena. Don't try to outlive it.
- **Bridge VLAN filtering.** StartWRT uses UCI `bridge-vlan` configuration for per-port VLAN assignment. OpenWrt maps this to DSA hardware switch tables when available (e.g., DeepComputing 4-port board) or to the kernel's software bridge filter when not (e.g., BPI-F3, which has two independent GMACs and no switch chip). The backend code is the same for both — it writes `NetworkBridgeVlan` / `NetworkVlanPort` sections regardless of the underlying hardware.

