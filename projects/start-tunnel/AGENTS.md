# AGENTS.md — start-tunnel

Practical instructions for working on the StartTunnel product inside the
`start-os` monorepo. Read the root `AGENTS.md` for monorepo-wide conventions
first; this file is scoped to `projects/start-tunnel/`. `CLAUDE.md` is a one-line
`@AGENTS.md` import. See [ARCHITECTURE.md](ARCHITECTURE.md) and
[CONTRIBUTING.md](CONTRIBUTING.md).

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## What this is

A WireGuard virtual private router with kernel-level clearnet port forwarding.
This directory is a **thin wrapper**: the binary entry point, the Angular UI, the
systemd unit, and the docs. The real logic lives in
`shared-libs/crates/start-core/src/tunnel/` (the `start_core::tunnel` module) and the
entry/CLI dispatch in `shared-libs/crates/start-core/src/bins/tunnel.rs`.

## Layout

- `src/main.rs` — embeds the UI, enables `start-tunnel`/`start-tunneld`, dispatches
  to `start-core`. Rarely needs changes.
- `Cargo.toml` — crate `start-tunnel`, bin `tunnelbox`. Depends on `start-core`.
- `start-tunneld.service` — systemd unit running `/usr/bin/start-tunneld`.
- `web/` — Angular project `start-tunnel` (registered in the root `angular.json`).
- `docs/` — mdbook (book title "StartTunnel"), output to `docs/book`.

Where to make changes:

| You want to change…                  | Edit…                                                            |
| ------------------------------------ | ---------------------------------------------------------------- |
| API methods / RPC commands           | `shared-libs/crates/start-core/src/tunnel/api.rs`                |
| db schema / state model              | `shared-libs/crates/start-core/src/tunnel/db.rs` (+ a migration) |
| WireGuard behavior                   | `shared-libs/crates/start-core/src/tunnel/wg.rs`                 |
| Port forwarding (DNAT, SNI, IGD/PCP) | `shared-libs/crates/start-core/src/tunnel/forward/`              |
| HTTPS / cert handling                | `shared-libs/crates/start-core/src/tunnel/web.rs`                |
| daemon startup / shutdown            | `shared-libs/crates/start-core/src/bins/tunnel.rs`               |
| the UI                               | `web/src/`                                                       |
| user docs                            | `docs/src/`                                                      |

Almost all backend work happens in `start-core`, not here. The one tunnel-local
Rust file is `src/main.rs`.

## Build & test (run from the repo root)

```bash
make start-tunnel                                   # build tunnelbox (UI + daemon)
cargo build -p start-tunnel --bin tunnelbox   # cargo only (UI must be prebuilt)
cargo check -p start-tunnel                    # fast type-check
npm run build:tunnel                          # build just the Angular UI (no make target; make start-tunnel chains it)
make start-tunnel-deb                                # build the .deb
make start-core-test                                 # backend tests (tunnel logic lives in start-core)
```

Notes:

- `cargo check`/`cargo build` here only cover the linux target. The CI matrix
  also builds `*-apple-darwin` and `riscv64`/`aarch64` musl — platform-specific
  code (`libc`, resolv-conf, etc.) can pass locally and break darwin. cfg-gate
  platform-only paths; don't reimplement them cross-platform.
- `make start-tunnel` needs the static UI at `web/dist/static/start-tunnel/index.html`;
  the Makefile target chains the UI build → `compress-uis.sh` automatically.
- TS bindings for the tunnel API regenerate via `make start-core-ts-bindings` into
  `shared-libs/crates/start-core/bindings/tunnel/`.

## Gotchas

- **One binary, three names.** `tunnelbox` is multi-call; it's installed as
  symlinks `start-tunneld` (daemon), `start-tunnel` (CLI), and `tunnelbox`. The
  active subcommand is chosen by `argv[0]`.
- **Default listen** is `127.0.59.60:5960` (`TUNNEL_DEFAULT_LISTEN` in
  `tunnel/mod.rs`). HTTPS is added/removed reactively from the `/webserver` db
  path — no restart on toggle.
- **State is PatchDB.** Changes are JSON patches; the daemon subscribes to db
  paths and reconciles kernel state (WireGuard, iptables). Schema changes need a
  numbered migration in `tunnel/migrations/` and registration in `mod.rs`.
- **Runtime deps.** The daemon shells out to `wireguard-tools`, `iptables`,
  `nftables`, and `conntrack`; the `.deb` declares them. Adding a new external
  tool means updating the `DEPENDS=` list in the Makefile `start-tunnel-deb` target.
- **Port forwarding is Layer 3/4.** It rewrites IP headers (DNAT) and does not
  decrypt payloads — keep it that way; TLS terminates at the user's service.
- **CLI and UI share `tunnel_api()`.** Add a method once in `api.rs`; both
  surfaces get it.
- **Version bump:** `Cargo.toml` `version` carries a `# VERSION_BUMP` marker —
  bump it there.
- **Manpages** for `start-tunnel` are generated (and committed) into this
  project's `man/` dir by `cargo test -p start-core export_manpage_start_tunnel`
  (the generator lives in `start-core`'s `bins/tunnel.rs`).

## Format

```bash
make start-tunnel-format        # format the tunnel Rust crate
make start-tunnel-format-check  # CI check (read-only)
```

The tunnel crate is Rust (edition 2024). The tunnel web app formats with the
rest of the Angular workspace via `make web-format`.

## Docs are part of the change

User-facing changes (UI, CLI flags/output, install flow, subnet/device/forward
behavior) must update `docs/src/` in the same change. The mdbook is published at
`start9.com/start-tunnel/`. Reference: `docs/src/cli-reference.md`,
`installing.md`, `subnets.md`, `devices.md`, `published-ports.md`.
