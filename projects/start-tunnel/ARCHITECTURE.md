# StartTunnel Architecture

StartTunnel is a WireGuard-based virtual private router with kernel-level
clearnet port forwarding. This document covers how the code is laid out across
the monorepo and how a request flows through the system. For the product-level
"what it is and how it compares" writeup, see
[`docs/src/architecture.md`](docs/src/architecture.md).

## Place in the monorepo

The `projects/start-tunnel/` directory is a thin product wrapper. The substance is in the
shared `start-core` crate.

| Concern                        | Location                                           |
| ------------------------------ | -------------------------------------------------- |
| Binary entry point             | `projects/start-tunnel/src/main.rs`                |
| Daemon + CLI dispatch          | `shared-libs/crates/start-core/src/bins/tunnel.rs` |
| Tunnel module root             | `shared-libs/crates/start-core/src/tunnel/mod.rs`  |
| Angular UI                     | `projects/start-tunnel/web/`                       |
| systemd unit                   | `projects/start-tunnel/start-tunneld.service`      |
| User & reference docs (mdbook) | `projects/start-tunnel/docs/`                      |

`src/main.rs` embeds the compiled UI (`web/dist/static/start-tunnel`) via
`include_dir!` into `start_core::tunnel::context::TUNNEL_UI_CELL`, then hands off to
`MultiExecutable`, enabling the `start-tunnel` (CLI) and `start-tunneld`
(daemon) subcommands. One binary, dispatched by `argv[0]` (busybox-style).

## The `start-core` tunnel module

All paths below are under `shared-libs/crates/start-core/src/tunnel/`.

- **`mod.rs`** — module root. Defines the default listen address
  (`127.0.59.60:5960`) and `tunnel_router`, which serves the embedded UI plus
  the API.
- **`context.rs`** — `TunnelContext` / `TunnelConfig`: process-wide state
  (database handle, listen address, shutdown channel) wired up at startup.
- **`db.rs`** — the PatchDB-backed state model: subnets, devices, port-forward
  entries, the webserver (HTTPS) config. State changes are RFC 6902 JSON
  patches; the daemon subscribes to paths (e.g. `/webserver`) and reacts.
- **`api.rs`** — the JSON-RPC API surface (rpc-toolkit). Every CLI command and
  every UI action is a method here: create/destroy subnets and devices, add and
  remove port forwards, manage the webserver and TLS.
- **`auth.rs`** — login / session auth for the web UI and CLI.
- **`web.rs`** — the embedded web server and `TunnelCertHandler` (on-demand TLS
  cert resolution for the HTTPS listener driven by the `/webserver` db path).
- **`wg.rs`** — WireGuard control: generates keys/PSKs, renders peer configs
  from the `*.conf.template` files, and applies interface state.
- **`dns.rs`** — DNS helpers for the tunnel network.
- **`forward/`** — the port-forwarding engine:
  - `mod.rs` — forward-entry orchestration (iptables/nftables DNAT rules).
  - `sni.rs` — SNI-based routing for forwarded TLS traffic.
  - `igd.rs` — IGD/UPnP upstream port mapping.
  - `pcp.rs` — PCP (Port Control Protocol) upstream port mapping.
- **`update.rs`** — self-update support for the daemon.
- **`migrations/`** — ordered db schema migrations (`m_00_*` …), registered in
  `migrations/mod.rs`.
- **`*.conf.template`** — WireGuard config templates (`server.conf`,
  `server-peer.conf`, `client.conf`).

## Daemon lifecycle (`bins/tunnel.rs`)

`start-tunneld` runs `inner_main` on a multi-threaded Tokio runtime:

1. Build `TunnelContext` from `TunnelConfig`.
2. Start a `WebServer` bound to the HTTP listen address, serving
   `tunnel_router` (UI + API).
3. Spawn a task subscribed to the `/webserver` db path. When HTTPS is enabled it
   binds a `TlsListener` with `TunnelCertHandler`; when disabled it tears the
   listener back down. This is fully reactive to db patches — no restart needed.
4. Spawn a signal handler (SIGINT/SIGQUIT/SIGTERM) that triggers graceful
   shutdown.
5. On shutdown, the return value can request a `reboot` or `poweroff`.

The `start-tunnel` CLI builds an `rpc-toolkit` `CliApp` against the same
`tunnel_api()`, so the CLI and UI share one API definition.

## Data flow: clearnet port forward

1. User adds a forward (UI or `start-tunnel`) → JSON-RPC method in `api.rs`.
2. The forward entry is written to PatchDB (`db.rs`).
3. The `forward/` engine reconciles kernel state: installs `iptables`/`nftables`
   DNAT rules so the VPS's public IP:port maps to the device's WireGuard IP:port.
4. Optionally requests an upstream mapping from the network gateway via
   IGD (`igd.rs`) or PCP (`pcp.rs`).
5. Inbound packets are NAT-forwarded at Layer 3/4 — payloads are never
   inspected, so TLS terminates at the destination service, not the tunnel.

## Frontend

The Angular app (`web/`) is the project `start-tunnel` in the shared Angular
workspace rooted at the repo root. It is **zoneless**, uses Taiga UI, and talks to the
daemon over the same JSON-RPC API.

- `web/src/app/app.routes.ts` — `home` (authed) vs `login` routes.
- `web/src/app/services/api/` — `LiveApiService` (real RPC) and
  `MockApiService`, selected by `useMocks` in the workspace `config.json`.
- `web/src/app/services/patch-db/` — PatchDB client; UI state mirrors the daemon
  db via the patch stream.
- `web/tsconfig.json` resolves `@start9labs/shared` and
  `@start9labs/marketplace` to `shared-libs/ts-modules/`.

Build output: `npm run build:tunnel` (from the repo root; `make start-tunnel`
chains it) → `projects/start-tunnel/web/dist/raw/start-tunnel/` → compressed to
`projects/start-tunnel/web/dist/static/start-tunnel/`
(`shared-libs/ts-modules/compress-uis.sh`), which the Rust binary embeds.

## Build & packaging

- `make start-tunnel` → `target/<arch>-unknown-linux-musl/<profile>/tunnelbox`
  (depends on the prebuilt static UI).
- `make start-tunnel-deb` → a Debian package declaring `wireguard-tools`, `iptables`,
  `nftables`, and `conntrack` as dependencies, installing the three symlinks and
  the systemd unit.
- TS bindings for the tunnel API are generated into
  `shared-libs/crates/start-core/bindings/tunnel/` (`make start-core-ts-bindings`).

## Further reading

- [`README.md`](README.md) — what StartTunnel is and how to use it.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — building, testing, and changing it.
- [`AGENTS.md`](AGENTS.md) — rules for AI agents working in this scope.
