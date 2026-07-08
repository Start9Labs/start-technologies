# StartTunnel

StartTunnel is a self-hosted **virtual private router (VPR)** — a minimal router
that runs on a VPS instead of sitting in your house. Like a home router, it
creates private networks over WireGuard, assigns IP addresses to devices, and
forwards ports to the public Internet. Because it lives in the cloud, it works
regardless of your home network's limitations (CGNAT, no public IP, ISP port
blocks).

Use it for private remote access to self-hosted services, or to expose services
to the clearnet without revealing your home IP address.

- **Clearnet hosting** like Cloudflare Tunnel, but you control the router and no
  third party sees your traffic.
- **Private access** like Tailscale, but fully self-hosted with no coordination
  server.
- **Layer 3/4 port forwarding** via kernel `iptables`/`nftables` DNAT — TLS
  terminates at your service, never at the tunnel.
- **MIT licensed**, no account, no telemetry, no terms of service.

For a feature tour and comparison to Cloudflare Tunnel / Tailscale, see the
[user documentation](docs/src/README.md).

## Place in the monorepo

StartTunnel is one product in the `start-os` monorepo. This directory is a thin
wrapper; the actual server logic lives in the shared Rust library.

```
projects/start-tunnel/
├── Cargo.toml              # crate "start-tunnel", bin "tunnelbox"
├── src/main.rs             # entry point: embeds the UI, dispatches to start-core
├── start-tunneld.service   # systemd unit (runs /usr/bin/start-tunneld)
├── web/                    # Angular UI (project "start-tunnel", registered in the root angular.json)
└── docs/                   # mdbook (book "StartTunnel")
```

- **Backend**: `shared-libs/crates/start-core/src/tunnel/` (module `start_core::tunnel`)
  holds the daemon, JSON-RPC API, WireGuard control, port-forward engine, DNS,
  auth, and the embedded web server.
- **CLI/daemon entry**: `shared-libs/crates/start-core/src/bins/tunnel.rs` provides
  both the `start-tunneld` daemon and the `start-tunnel` CLI.
- **Frontend**: the Angular app under `web/` references the shared libs
  `@start9labs/shared` and `@start9labs/marketplace` from `shared-libs/ts-modules/`.

The single `tunnelbox` binary is installed as three symlinks:

| Command         | Role                              |
| --------------- | --------------------------------- |
| `start-tunneld` | the long-running daemon (systemd) |
| `start-tunnel`  | the management CLI / RPC client   |
| `tunnelbox`     | the multi-call binary itself      |

## Quickstart

### Install on a VPS

End users install a release build with the hosted script:

```bash
curl -fsSL https://start9.com/start-tunnel/install.sh | sudo bash
```

See [docs/src/installing.md](docs/src/installing.md) for the full walkthrough
(VPS requirements, login, creating your first subnet and device).

### Build from source

All builds run from the **repo root**, not this directory.

```bash
# Build the daemon binary (musl target, embeds the prebuilt UI)
make start-tunnel

# Build just the Angular UI (no make target; make start-tunnel chains it)
npm run build:tunnel

# Build the cargo binary directly (UI must already be built)
cargo build -p start-tunnel --bin tunnelbox

# Build a .deb package
make start-tunnel-deb
```

`make start-tunnel` produces
`target/<arch>-unknown-linux-musl/<profile>/tunnelbox`, which `src/main.rs`
embeds the compiled UI from `web/dist/static/start-tunnel/` into.

## Requirements

- Debian 13
- x86_64, aarch64, or riscv64
- Root access
- `wireguard-tools`, `iptables`, `nftables`, `conntrack` (pulled in by the .deb)
- A public IP (required only for clearnet port forwarding)

## Documentation & Contributing

- User & reference docs: [`docs/`](docs/) (browse at
  [start9.com/start-tunnel](https://start9.com/start-tunnel/))
- Architecture: [ARCHITECTURE.md](ARCHITECTURE.md)
- Building, testing, formatting: [CONTRIBUTING.md](CONTRIBUTING.md)
- Agent/dev quick reference: [AGENTS.md](AGENTS.md)

## License

MIT. See [LICENSE](../../LICENSE).
