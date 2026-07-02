# Architecture

StartWRT is an OpenWrt-based router OS for home self-hosting. It pairs a Rust backend with an Angular frontend to expose advanced networking — per-device security profiles, VPN chaining, WiFi schedules — through a clean web interface.

## Tech Stack

- **Backend:** Rust (async/Tokio, Axum web framework)
- **Frontend:** Angular 22 + TypeScript 5.9 + Taiga UI v5
- **Router OS:** OpenWrt (SpacemiT K1 / BananaPi F3 target)
- **Config storage:** UCI files (`/etc/config/`) — no separate database
- **API:** JSON-RPC 2.0 over HTTP POST at `/rpc/v1`
- **Auth:** Password + session cookie (SHA-256 hashed tokens, 1-day expiry)

## Project Structure

```
/
├── backend/             # Rust workspace (3 crates)
│   ├── ctrl/            # RPC server + CLI binary ("startwrt")
│   ├── uciedit/         # UCI config parser/writer library
│   ├── uciedit_macros/  # #[derive(TypedSection)] proc macro
│   ├── firstboot_config/# Factory-default UCI configs (embedded in binary)
│   └── config_experiments/ # Reference UCI configs for testing
│
├── web/                 # Angular 22 SPA
│   └── src/app/
│       ├── services/    # API, auth, form, RPC, system
│       ├── components/  # Shared UI (footer, masked, copy, etc.)
│       ├── routes/      # Feature pages (wan, wifi, profiles, etc.)
│       └── utils/       # Validators, masks, schedules
│
├── openwrt/             # OpenWrt fork (git submodule, branch: bianbu)
├── build/               # Build scripts and OpenWrt diffconfig
├── docs/                # User-facing docs book (src/, book.toml) + cross-cutting specs/proposals
└── API_CONTRACT.md      # Complete RPC endpoint contract with Rust types
```

## Components

- **`backend/`** — Rust daemon and CLI. Produces a single binary `startwrt` that is symlinked as `startwrt-ctrld` (daemon) and `startwrt-cli` (CLI). Handles all backend logic: RPC API, security profiles, WiFi, Ethernet, VPN, authentication, and UCI config management. See [backend/ARCHITECTURE.md](backend/ARCHITECTURE.md).

- **`web/`** — Angular 22 SPA using Taiga UI v5. Signal-based state, zoneless change detection, standalone components. Communicates with the backend exclusively via JSON-RPC 2.0. Embeds contextual help on every page. See [web/ARCHITECTURE.md](web/ARCHITECTURE.md).

- **`openwrt/`** — Git submodule pointing to Start9's OpenWrt fork (SpacemiT K1 target). The build system compiles the Rust backend + Angular frontend, stages them into `openwrt/files/`, and produces a flashable image.

## Data Flow

```
Browser (Angular SPA)
  → HTTP POST /rpc/v1  (JSON-RPC 2.0, session cookie)
  → Axum router on port 80/443
  → SessionAuth middleware (validates cookie, rate-limits login)
  → rpc-toolkit handler dispatch
  → Handler reads/writes UCI files via uciedit (arena-based, atomic writes)
  → Handler calls /etc/init.d/<service> reload to apply changes
  → JSON-RPC response back to browser
```

Additional HTTP routes:
- `GET /api/logs` — WebSocket for live log streaming
- `POST /api/setup/flash` — NDJSON streaming for setup wizard
- `GET|POST /rest/rpc/{guid}` — Continuation endpoint for backup/restore/diagnostics
- `GET /static/root-ca.crt` — Root CA download (no auth)
- `/cgi-bin/*`, `/luci-static/*`, `/ubus/*` — LuCI reverse proxy (localhost:8080)
- Fallback — Serves embedded web UI

## Security Profiles

The core concept. Each profile creates:

1. A **VLAN** on the LAN bridge (`br-lan`) with a unique tag
2. A **network interface** bound to that VLAN with its own subnet
3. A **firewall zone** with forwarding rules governing LAN/WAN access
4. A **DHCP server** for the profile's subnet

Devices receive a profile based on how they join the network:

| Entry Point | Mechanism |
|-------------|-----------|
| **WiFi** | One SSID, many passwords. Each password maps to a profile via `wpa_psk_file` with dynamic VLAN. |
| **Ethernet** | Bridge VLAN port assignments — each physical port tagged to a profile's VLAN. OpenWrt maps UCI `bridge-vlan` config to DSA hardware tables or software bridge filtering depending on the board. |
| **Inbound VPN** | Each WireGuard server interface bound to a profile. |

Profile orchestration spans four UCI configs: `startwrt`, `network`, `firewall`, `dhcp`.

## Build Pipeline

Targets live in `build.mk` (included by the root `Makefile`) and run from the **monorepo
root**. The Rust crates are members of the root Cargo workspace, so the binary lands in the
workspace-root `target/`. The riscv64 binary is cross-compiled in a dockerized cargo-zigbuild
toolchain pinned to the SpaceMiT K1 ISA (`build/build-rust.sh` + `build/zigcc-k1.sh`).

### Build Steps

```
1. npm run build:wrt               →  web/dist/startwrt/browser/ (Angular production build, embedded next)
2. build/build-rust.sh             →  target/riscv64gc-unknown-linux-musl/release/startwrt
3. build/stage-files.sh            →  openwrt/files/ (binary + configs + init scripts)
4. make -C openwrt                 →  openwrt/bin/targets/spacemit/*.img
5. cp to results/                  →  Final flashable image
```

### Key Make Targets

| Target | Description |
|--------|-------------|
| `make startwrt` | web → Rust binary (embeds the UI) |
| `make startwrt-image` | Full build: stage → OpenWrt image → `results/` |
| `make startwrt-openwrt-setup` | One-time: configure feeds, download packages |
| `make startwrt-update STARTWRT_REMOTE=root@IP` | Deploy binary over SSH (atomic: temp → sync → rename → restart) |
| `make clean-startwrt` | Delete start-wrt build artifacts |

### Deployment

The `startwrt-update` target pipes the binary over SSH with atomic replacement:

```bash
make startwrt-update                        # Default: root@192.168.0.1
make startwrt-update STARTWRT_REMOTE=root@10.0.0.1   # Custom target
```

The binary is self-contained — the web UI is embedded via `include_dir`. Factory-default UCI configs from `firstboot_config/` are staged into the OpenWrt image at build time by `build/stage-files.sh`.

## Key Design Decisions

1. **One SSID, many passwords.** Per-password VLAN assignment via `wpa_psk_file` with dynamic VLAN.

2. **VLANs for isolation.** Layer 2 isolation via bridge VLAN filtering. On boards with a hardware switch (DSA), filtering happens in silicon; on boards without (BPI-F3), it's done in the kernel's software bridge. The UCI `bridge-vlan` configuration is the same either way. An earlier nftables approach was abandoned because in-hardware switching bypasses the OS.

3. **UCI as source of truth.** No separate database. The backend reads, modifies, and writes UCI files atomically with conflict detection.

4. **Frontend knows nothing about UCI.** Clean RPC contract where the frontend sends domain objects and the backend handles all UCI serialization, service restarts, and system commands.

5. **Typed UCI parsing.** `#[derive(TypedSection)]` eliminates runtime parsing errors. Define a struct, annotate with `#[uci(ty = "...")]`, derive, done.

6. **Single binary.** `startwrt` is symlinked as `startwrt-ctrld` (daemon) and `startwrt-cli` (CLI). Both share handler code via the `CtrlContext` trait.

## Further Reading

- [backend/ARCHITECTURE.md](backend/ARCHITECTURE.md) — Rust backend internals
- [web/ARCHITECTURE.md](web/ARCHITECTURE.md) — Angular frontend internals
- [API_CONTRACT.md](API_CONTRACT.md) — Complete RPC endpoint contract
- [docs/init-reflash.md](docs/init-reflash.md) — Manufacturing, setup, and reflash flows
