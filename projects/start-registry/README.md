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

## Private packages

Admins can restrict package metadata and icons to admins, authorized publishers, and selected
registered signers:

```bash
start-registry package access set-private <PACKAGE_ID> true
start-registry package access add-reader <PACKAGE_ID> <SIGNER_ID>
start-registry package access get <PACKAGE_ID>
start-registry package access remove-reader <PACKAGE_ID> <SIGNER_ID>
```

`set-private` creates a package entry when needed, allowing restriction before publication.
Existing packages default to public. Readers authenticate through encrypted sessions or signed
requests; revocations update active subscriptions. A subscriber using the public mirror must reconnect after receiving a private
access grant. Archive URLs remain governed by their download host.

## Package endorsements

Package add and add-mirror import the hosted archive's embedded signature and a submitted `signatures` map. The CLI reads verified endorsements from its local v2 S9PK and submits that map separately from the authenticated uploader's signature. Promotion forwards the existing indexed map. Every submitted endorsement must verify against the exact indexed Merkle root, signed bound, and `s9pk` context; an invalid entry rejects the request atomically. Hosted URLs can serve the base archive alone. Source URLs and bytes remain unchanged.

Supplemental keys enter the asset's signature map, not the registry signer directory or trust proposal. Publishing authorization still belongs to the registered uploader. Operators can prepare an endorsed file with `start-cli s9pk sign <file.s9pk>` and pass it to `registry package add` or `add-mirror`.

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

| Flag                    | Purpose                                                 |
| ----------------------- | ------------------------------------------------------- |
| `-l, --listen <addr>`   | listen address (default `127.0.0.1:5959`)               |
| `-H, --hostname <host>` | public hostname(s) for asset URLs and authenticated RPC |
| `-p, --tor-proxy <url>` | SOCKS proxy for outbound (e.g. Tor) requests            |
| `-d, --datadir <path>`  | data directory (default `/var/lib/startos`)             |
| `-c, --config <path>`   | config file                                             |

Clients synchronize registry metadata through `db.subscribe`: `rpcstream` opt-in uses the shared `/rpc/v2` socket, while ordinary requests retain the GUID WebSocket continuation. StartOS authenticates and encrypts this shared connection through signed ephemeral DH. Streaming requires encrypted admission and `rpcstream`, with the client's selected URL host configured on the registry. A legacy HTTP 200 response without upgrading, or HTTP 404/405/501, selects signed HTTP polling every 15 minutes; each cycle retries upgrade support. Polls retrieve full package versions/revisions, registry info and the selected OS index through the same cache/trust owners. Authorization, transport and failed encrypted admission errors never downgrade. A subscription's optional `osVersion` selects a lazily shared backward-compatible index view; omitted versions retain the canonical format. StartOS maintains package, signer, and OS-update caches independently of open browser sessions. OS releases marked `security` generate notifications on compatible StartOS servers using that registry for updates.

Ordinary RPC requests have a 1 MiB encoded-body limit, including JSON and base64 overhead. This also bounds inline registry icons sent by `registry info set-icon`; use a smaller image when its encoded request exceeds that budget. Large index responses and asset downloads retain their separate limits.

Server state lives in `<datadir>/registry.db` (PatchDB) plus a SQLite metrics database and hosted asset files.

> [!NOTE]
> **Non-StartOS deployments only:** A separately managed reverse proxy must forward `/`, `/rpc/`,
> `/rpc/v2`, `/rpc/v2/`, `/ws/rpc/` (legacy continuations), `/rest/rpc/`, and `/icons/`. Preserve the public `Host` header and set
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
