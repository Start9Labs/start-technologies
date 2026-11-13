# start-registry

The **Start Registry** server — a self-hostable registry for StartOS packages (`.s9pk`) and StartOS itself (OS images / version index). It is what a marketplace UI (the `@start9labs/marketplace` Angular library) points at when browsing, searching, and downloading services and OS releases.

This directory is a thin product wrapper inside the `start-technologies` monorepo. The crate compiles a single multi-call binary, `registrybox`; the actual server and CLI logic live in the shared `start-core` crate (`shared-libs/crates/start-core/src/registry`).

## What it does

- Serves a **package index** (services, versions, dependency ranges, assets) and an **OS index** (StartOS versions and downloadable images).
- Hosts `.s9pk` and OS image **assets** with content-addressed (Blake3) commitments.
- Redirects visitors at `/` to `https://marketplace.start9.com/` with the registry selected.
- Exposes a JSON-RPC API over HTTP, WebSocket, and a REST continuation channel.
- Tracks **admins** and package **signers**, verifies signatures on uploaded content, and proposes administrator-selected signers for Marketplace trust when users connect.
- Records anonymous **metrics** (downloads / users) for admin reporting.

## Package trust proposals

Pass an armored GPG public-key export to `admin signer add --key` or `admin signer edit --add-key` to retain its complete certificate, identities, and fingerprints. Signers stored with only DER keys need their public certificate reimported to supply this information.

Choose which existing signers to propose when a user connects this registry:

```bash
start-registry admin signer list
start-registry info set-trust-signers <SIGNER_ID> [SIGNER_ID...]
```

Run `start-registry info set-trust-signers` without IDs to clear the selection. Registries start with no proposed signers. `registry.info` exposes the selected signer IDs and their complete signer records as `trustSigners`, including public keys and preserved GPG exports. Connecting a registry in StartOS trusts the offered keys it does not already have for packages. Keys added later are imported untrusted when StartOS next fetches packages or OS updates from the registry, with a notification for each that offers to trust it. Choosing the registry for StartOS updates trusts its offered keys for StartOS updates.

## Package endorsements

Package add and add-mirror import the hosted archive's embedded signature and a submitted `signatures` map. The CLI reads verified endorsements from its local v2 S9PK and submits that map separately from the authenticated uploader's signature. Promotion forwards the existing indexed map. Every submitted endorsement must verify against the exact indexed Merkle root, signed bound, and `s9pk` context; an invalid entry rejects the request atomically. Hosted URLs can serve the base archive alone. Source URLs and bytes remain unchanged.

Supplemental keys enter the asset's signature map, not the registry signer directory or trust proposal. Publishing authorization still belongs to the registered uploader. Operators can prepare an endorsed file with `start-cli s9pk sign <file.s9pk>` and pass it to `registry package add` or `add-mirror`.

## OS release metadata

Administrators can update an existing release without re-publishing its images:

```bash
start-registry os version edit 0.4.0.5 --security false
start-cli registry os version edit 0.4.0.5 --release-notes '{"en_US":"Updated notes","pl_PL":"Zaktualizowane informacje"}'
```

Optional flags are `--release-notes`, `--source-version`, `--git-hash`, and `--security true|false`. Omitted fields remain unchanged, including image assets and authorized signers. Release notes accept plain text or a locale map and replace the complete existing value. The version must already exist. Use `--expected-git-hash <commit>` to reject the edit if the indexed image commit has changed.

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

| Flag                       | Purpose                                         |
| -------------------------- | ----------------------------------------------- |
| `-l, --listen <addr>`      | listen address (default `127.0.0.1:5959`)       |
| `-H, --hostname <host>`    | public hostname(s) used in generated asset URLs |
| `-p, --tor-proxy <url>`    | SOCKS proxy for outbound (e.g. Tor) requests    |
| `-d, --datadir <path>`     | data directory (default `/var/lib/startos`)     |
| `-c, --config <path>`      | config file                                     |
| `--security-notifications` | enable the OS security feed (default: false)    |

Security notifications are disabled by default. To opt in, set this in that registry's config file and restart the daemon:

```yaml
registry-hostname: [registry.example.com]
security-notifications: true
```

Disabled registries refuse `/os/security` upgrades with HTTP 403 and create no security mirror or subscription task. Clients retry with backoff. Leave alpha at the default to suppress its announcements. Release metadata and promotion retain the security flag; previously issued OS notifications are not deleted.

Server state lives in `<datadir>/registry.db` (PatchDB) plus a SQLite metrics database and hosted asset files.

> [!NOTE]
> **Non-StartOS deployments only:** A separately managed reverse proxy must forward `/`, `/rpc/`,
> `/ws/rpc/`, `/rest/rpc/`, `/icons/`, and `/os/security`. Preserve the public `Host` header and set
> `X-Forwarded-Proto` to the public scheme (`http` or `https`) for the root redirect. The WebSocket routes also need HTTP/1.1 upgrade forwarding and a
> long-lived idle timeout. StartOS-hosted registries expose these routes through their exported
> `protocol: 'http'` interface automatically.

## Where things are

- `Cargo.toml` — crate `start-registry`, bin `registrybox`, depends on `start-core` (package name `start-core`, lib `start_core`).
- `src/main.rs` — wires up the `MultiExecutable` (`enable_start_registry` + `enable_start_registryd`).
- `start-registryd.service` — systemd unit.
- Server/CLI/API/data model: `shared-libs/crates/start-core/src/registry/`.
- Marketplace UI library: `shared-libs/ts-modules/marketplace/` (`@start9labs/marketplace`).

## Documentation

See [ARCHITECTURE.md](./ARCHITECTURE.md) for module-level detail and [AGENTS.md](./AGENTS.md) for build/test and contributor instructions.
