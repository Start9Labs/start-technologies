# start-registry

The **Start Registry** server — a self-hostable registry for StartOS packages (`.s9pk`) and StartOS itself (OS images / version index). It is what a marketplace UI (the `@start9labs/marketplace` Angular library) points at when browsing, searching, and downloading services and OS releases.

This directory is a thin product wrapper inside the `start-technologies` monorepo. The crate compiles a single multi-call binary, `registrybox`; the actual server and CLI logic live in the shared `start-core` crate (`shared-libs/crates/start-core/src/registry`).

## What it does

- Serves a **package index** (services, versions, dependency ranges, assets) and an **OS index** (StartOS versions and downloadable images).
- Records HTTP mirrors and optional BitTorrent sources for `.s9pk` and OS image assets, alongside their signed commitments.
- Exposes a JSON-RPC API over HTTP, WebSocket, and a REST continuation channel.
- Tracks **admins** and package **signers**, and verifies signatures on indexed assets.
- Records anonymous **metrics** (downloads / users) for admin reporting.

## Binary layout

`registrybox` is a multi-call binary (like BusyBox). It dispatches on the name it is invoked as:

| Invoked as        | Role                                                                   |
| ----------------- | ---------------------------------------------------------------------- |
| `start-registryd` | the registry **server** daemon (`registry::main`)                      |
| `start-registry`  | the registry **CLI** client against a running server (`registry::cli`) |

The Debian package installs the binary as `/usr/bin/start-registrybox` and symlinks `start-registryd` and `start-registry` to it. The `start-registryd.service` systemd unit runs the daemon.

## Quickstart

Build and run from the monorepo root:

```bash
# build the binary
cargo build -p start-registry --bin registrybox

# run the server (defaults: listen 127.0.0.1:5959, datadir /var/lib/startos)
./target/debug/registrybox start-registryd --listen 0.0.0.0:5959 --datadir ./registry-data

# talk to it with the CLI
./target/debug/registrybox start-registry --help
```

Configuration is read from CLI flags and a config file (`-c <path>`; default search includes the standard StartOS config path). Key flags:

| Flag                       | Purpose                                        |
| -------------------------- | ---------------------------------------------- |
| `-l, --listen <addr>`      | listen address (default `127.0.0.1:5959`)      |
| `-H, --hostname <host>`    | public hostname(s) used for request signatures |
| `-p, --tor-proxy <url>`    | SOCKS proxy for outbound (e.g. Tor) requests   |
| `-d, --datadir <path>`     | data directory (default `/var/lib/startos`)    |
| `-c, --config <path>`      | config file                                    |
| `--security-notifications` | enable the OS security feed (default: false)   |

Security notifications are disabled by default. To opt in, set this in that registry's config file and restart the daemon:

```yaml
registry-hostname: [registry.example.com]
security-notifications: true
```

Disabled registries refuse `/os/security` upgrades with HTTP 403 and create no security mirror or subscription task. Clients retry with backoff. Leave alpha at the default to suppress its announcements. Release metadata and promotion retain the security flag; previously issued OS notifications are not deleted.

Server state lives in `<datadir>/registry.db` (PatchDB), a SQLite metrics database, and the package icon store.

> [!NOTE]
> **Non-StartOS deployments only:** A separately managed reverse proxy must forward `/rpc/`,
> `/ws/rpc/`, `/rest/rpc/`, `/icons/`, and `/os/security`. The WebSocket routes also need HTTP/1.1 upgrade forwarding and a
> long-lived idle timeout. StartOS-hosted registries expose these routes through their exported
> `protocol: 'http'` interface automatically.

## Publishing BitTorrent sources

Publish an external magnet link or HTTP(S) `.torrent` metadata URL with `--torrent`, alongside the HTTP download URL:

```sh
start-cli --registry https://registry.example.com registry package add service.s9pk \
  --url https://downloads.example.com/service.s9pk \
  --torrent 'magnet:?xt=urn:btih:<infohash>&tr=<tracker-url>'
```

`registry os asset add` accepts the same `--torrent` option for OS images. The RPC `package.add` and `os.asset.add.*` requests accept an optional `torrent` string. Package and OS promotion preserve it and the HTTP mirrors.

The torrent must contain a single file with the exact bytes of the HTTP asset. Registry operators arrange seeding through an external BitTorrent client, with reachable seed peers and tracker or DHT discovery. The registry stores the source metadata; asset storage and seeding belong to those external hosts.

StartOS and local `start-cli` downloads prefer BitTorrent and fall back to HTTP when it is unavailable or fails commitment verification. Existing signature requirements apply to either transport. Older clients ignore the optional metadata and continue using HTTP.

## Where things are

- `Cargo.toml` — crate `start-registry`, bin `registrybox`, depends on `start-core` (package name `start-core`, lib `start_core`).
- `src/main.rs` — wires up the `MultiExecutable` (`enable_start_registry` + `enable_start_registryd`).
- `start-registryd.service` — systemd unit.
- Server/CLI/API/data model: `shared-libs/crates/start-core/src/registry/`.
- Marketplace UI library: `shared-libs/ts-modules/marketplace/` (`@start9labs/marketplace`).

## Documentation

See [ARCHITECTURE.md](./ARCHITECTURE.md) for module-level detail and [AGENTS.md](./AGENTS.md) for build/test and contributor instructions.
