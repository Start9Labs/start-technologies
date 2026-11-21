# start-registry

The **Start Registry** server — a self-hostable registry for StartOS packages (`.s9pk`) and StartOS itself (OS images / version index). It is what a marketplace UI (the `@start9labs/marketplace` Angular library) points at when browsing, searching, and downloading services and OS releases.

This directory is a thin product wrapper inside the `start-technologies` monorepo. The crate compiles a single multi-call binary, `registrybox`; the actual server and CLI logic live in the shared `start-core` crate (`shared-libs/crates/start-core/src/registry`).

## What it does

- Serves a **package index** (services, versions, dependency ranges, assets) and an **OS index** (StartOS versions and downloadable images).
- Hosts `.s9pk` and OS image **assets** with content-addressed (Blake3) commitments.
- Redirects visitors at `/` to `https://marketplace.start9.com/` with the registry selected.
- Exposes a JSON-RPC API over HTTP, WebSocket, and a REST continuation channel.
- Tracks **admins** and package **signers**, verifies signatures on uploaded content, and proposes administrator-selected signers for Marketplace trust when users connect.
- Records anonymous **metrics** for its admins: installs and updates StartOS reports (package and version only), active servers from OS update checks, index subscriptions, and package lookups, each as totals or a day/week/month series (`start-registry metrics`).

## Package trust proposals

Pass an armored GPG public-key export to `admin signer add --key` or `admin signer edit --add-key` to retain its complete certificate, identities, and fingerprints. Signers stored with only DER keys need their public certificate reimported to supply this information.

Choose which existing signers to propose when a user connects this registry:

```bash
start-registry admin signer list
start-registry info set-trust-signers <SIGNER_ID> [SIGNER_ID...]
```

Run `start-registry info set-trust-signers` without IDs to clear the selection. Registries start with no proposed signers. `registry.info` exposes the selected signer IDs and their complete signer records as `trustSigners`, including public keys and preserved GPG exports. Connecting a registry in StartOS trusts the offered keys it does not already have for packages. Keys added later are imported untrusted when StartOS next fetches packages or OS updates from the registry, with a notification for each that offers to trust it. Choosing the registry for StartOS updates trusts its offered keys for StartOS updates.

## Private packages

A private s9pk is hosted encrypted. The registry stores its AES-256-CTR keystream and shows the
s9pk, keystream included, only to admins, the package's authorized publishers, and the reader keys
it lists. Versions and packages with no s9pk a requester can see are hidden from them. Admins keep
a labeled directory of reader keys, typically StartOS server keys:

```bash
start-registry admin reader add <LABEL> <KEY>
start-registry admin reader list
start-registry admin reader edit <ID> --label <LABEL>
start-registry admin reader remove <ID>
```

Publishers mark an s9pk private when indexing it, listing each reader key:

```bash
start-cli s9pk publish <FILE> --private --reader <KEY>...
start-cli s9pk encrypt <FILE>
start-cli registry package add <FILE> --url <URL> --private --reader <KEY>...
```

`s9pk publish --private` uploads ciphertext to S3. `s9pk encrypt` writes the ciphertext for hosting
elsewhere, and `registry package add --private` indexes it. The keystream is derived from the
publisher's key and the archive, so both commands agree. Clients decrypt as they download and
verify the archive's commitment. Readers authenticate through encrypted sessions or signed
requests; revocations update active subscriptions. A subscriber using the public mirror must
reconnect after receiving a private access grant.

## Package endorsements

Package add and add-mirror import the hosted archive's embedded signature and a submitted `signatures` map. The CLI reads verified endorsements from its local v2 S9PK and submits that map separately from the authenticated uploader's signature. Promotion forwards the existing indexed map. Every submitted endorsement must verify against the exact indexed Merkle root, signed bound, and `s9pk` context; an invalid entry rejects the request atomically. Hosted URLs can serve the base archive alone. Source URLs and bytes remain unchanged.

Supplemental keys enter the asset's signature map, not the registry signer directory or trust proposal. Publishing authorization still belongs to the registered uploader. Operators can prepare an endorsed file with `start-cli s9pk sign <file.s9pk>` and pass it to `registry package add` or `add-mirror`.

## OS release metadata

Administrators can update an existing release without re-publishing its images:

```bash
start-registry os version edit 0.4.0.5 --security false
start-cli registry os version edit 0.4.0.5 --release-notes '{"en_US":"Updated notes","pl_PL":"Zaktualizowane informacje"}'
```

Optional flags are `--release-notes`, `--source-version`, `--git-hash`, and `--security true|false`. Omitted fields remain unchanged, including image assets and authorized signers. Release notes accept plain text or a locale map and replace the complete existing value. The version must already exist. Use `--expected-git-hash <commit>` to reject the edit if the indexed image commit has changed. Mirrors reject this command; edit the upstream registry instead.

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
| `--mirror-of <url>`     | mirror another registry's index (read-only index)       |

Clients synchronize registry metadata through `db.subscribe`: `rpcstream` opt-in uses the shared `/rpc/v2` socket, while ordinary requests retain the GUID WebSocket continuation. StartOS authenticates and encrypts this shared connection through signed ephemeral DH. Streaming requires encrypted admission and `rpcstream`, with the client's selected URL host configured on the registry. A legacy HTTP 200 response without upgrading, or HTTP 404/405/501, selects signed HTTP polling every 15 minutes; each cycle retries upgrade support. Polls retrieve full package versions/revisions, registry info and the selected OS index through the same cache/trust owners. Authorization, transport and failed encrypted admission errors never downgrade. A subscription's optional `osVersion` selects a lazily shared backward-compatible index view, and its optional `language` a lazily shared view with every translated field resolved to that locale; omitting them retains the canonical format. StartOS maintains package, signer, and OS-update caches independently of open browser sessions. OS releases marked `security` generate notifications on compatible StartOS servers using that registry for updates.

## Mirroring another registry

A registry started with `--mirror-of <url>` (or `mirror-of: <url>` in its config file) keeps a synchronized copy of the upstream index visible to its identity. It follows the upstream's `/index` subscription and copies the package icons that index refers to. Index changes appear on the mirror as the upstream publishes them. The upstream must support encrypted `db.subscribe` streaming. Package and OS files stay at the URLs the index lists.

The mirror subscribes with its own server identity, logged as `key` when mirroring starts. It receives what the upstream shows that identity: public packages to an unknown identity, private packages it may read once the upstream adds it as a signer and reader, and reader lists only when that signer is an upstream admin. Restart a public-only mirror after granting its identity private-package access or upstream admin status. For failover with private-package reader lists intact, make the mirror an upstream admin.

While mirroring, the registry rejects every call that writes the index: publishing, signers, categories, access, registry info and `db apply`. Its admins, metrics and identity stay its own. To promote a mirror, remove `mirror-of` and restart it. It keeps the last synchronized index and accepts writes again. Admins refer to signer IDs, so add the upstream's admins on the mirror with `start-registry admin add <signer-id>` before promotion.

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
