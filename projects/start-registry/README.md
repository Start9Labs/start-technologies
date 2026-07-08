# start-registry

The **Start Registry** server — a self-hostable registry for StartOS packages (`.s9pk`) and StartOS itself (OS images / version index). It is what a marketplace UI (the `@start9labs/marketplace` Angular library) points at when browsing, searching, and downloading services and OS releases.

This directory is a thin product wrapper inside the `start-os` monorepo. The crate compiles a single multi-call binary, `registrybox`; the actual server and CLI logic live in the shared `start-core` crate (`shared-libs/crates/start-core/src/registry`).

## What it does

- Serves a **package index** (services, versions, dependency ranges, assets) and an **OS index** (StartOS versions and downloadable images).
- Hosts `.s9pk` and OS image **assets** with content-addressed (Blake3) commitments.
- Exposes a JSON-RPC API over HTTP, WebSocket, and a REST continuation channel.
- Tracks **admins** and package **signers**, and verifies signatures on uploaded content.
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

| Flag                    | Purpose                                         |
| ----------------------- | ----------------------------------------------- |
| `-l, --listen <addr>`   | listen address (default `127.0.0.1:5959`)       |
| `-H, --hostname <host>` | public hostname(s) used in generated asset URLs |
| `-p, --tor-proxy <url>` | SOCKS proxy for outbound (e.g. Tor) requests    |
| `-d, --datadir <path>`  | data directory (default `/var/lib/startos`)     |
| `-c, --config <path>`   | config file                                     |

Server state lives in `<datadir>/registry.db` (PatchDB) plus a SQLite metrics database and hosted asset files.

## Where things are

- `Cargo.toml` — crate `start-registry`, bin `registrybox`, depends on `start-core` (package name `start-core`, lib `start_core`).
- `src/main.rs` — wires up the `MultiExecutable` (`enable_start_registry` + `enable_start_registryd`).
- `start-registryd.service` — systemd unit.
- Server/CLI/API/data model: `shared-libs/crates/start-core/src/registry/`.
- Marketplace UI library: `shared-libs/ts-modules/marketplace/` (`@start9labs/marketplace`).

## Documentation

See [ARCHITECTURE.md](./ARCHITECTURE.md) for module-level detail, [AGENTS.md](./AGENTS.md) for build/test instructions, and [CONTRIBUTING.md](./CONTRIBUTING.md) for the contribution workflow.
