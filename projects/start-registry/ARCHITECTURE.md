# Architecture — start-registry

`start-registry` is one of the product wrappers in the `start-technologies` monorepo. The wrapper itself is tiny; nearly all logic lives in the shared backend crate `start-core` (`shared-libs/crates/start-core`, package `start-core`, lib name `start_core`).

## Place in the monorepo

`start-registry` lives at `projects/start-registry/` and is the product wrapper for the registry server/CLI (the `registrybox` bin + systemd unit). Its real implementation lives in `shared-libs/crates/start-core` (`src/bins/registry.rs` + `src/registry/`), and its browsing UI is `@start9labs/marketplace` (`shared-libs/ts-modules/marketplace/`). See the root [`ARCHITECTURE.md`](../../ARCHITECTURE.md) for the overall monorepo layout.

## Binary: `registrybox`

`src/main.rs` builds a `MultiExecutable` (from `start_core::bins`) and registers two entry points:

```rust
MultiExecutable::default()
    .enable_start_registry()   // "start-registry"  -> registry::cli  (CLI client)
    .enable_start_registryd()  // "start-registryd" -> registry::main (server daemon)
    .execute()
```

`MultiExecutable::execute` inspects `argv[0]` (and `argv[1]`) to choose which entry point to run, so the single compiled `registrybox` behaves as either binary depending on the name it's launched under. Installation (root `Makefile` `start-registry-install`) places the binary at `/usr/bin/start-registrybox` and symlinks `start-registryd` and `start-registry` to it.

The dispatch table also responds to `--contents` (lists the embedded sub-binaries), which the packaging tooling uses.

## Server (`start-registryd` → `registry::main`)

Defined in `shared-libs/crates/start-core/src/bins/registry.rs`:

1. Parse `RegistryConfig` (CLI flags + config files) and `load()` it.
2. Build a multi-threaded Tokio runtime.
3. `RegistryContext::init` — open/migrate the database, create the data dir, set up the HTTP client (optionally via a Tor SOCKS proxy), auth cookie, and metrics DB.
4. Start a `WebServer` bound to the configured listen address, serving `registry_router(ctx)`.
5. Install SIGINT/SIGQUIT/SIGTERM handlers that broadcast a shutdown, then drain the server gracefully.

## HTTP routing (`registry_router`)

Defined in `shared-libs/crates/start-core/src/registry/mod.rs`:

- `GET /` — HTTP 307 to `https://marketplace.start9.com/?registry=<encoded-origin>`, using the request authority and `X-Forwarded-Proto` (plain HTTP by default).
- `POST /rpc/{*path}` — JSON-RPC through outer CORS and shared `Auth` (local + signature auth) and `DeviceInfoMiddleware`.
- `GET /rpc/v2` — encrypted multiplexed JSON-RPC and opt-in `rpcstream` responses. Signed ephemeral-DH admission uses two consecutive CBOR objects per hello, without URL selectors. Valid unknown keys can call public methods; per-method authorization observes current enrollment and permissions on the same open connection. Upgrade headers cannot replace its verified peer identity.
- `GET /rpc/v2/{*path}` — WebSocket RPC continuations, keyed by a `Guid`; `/ws/rpc/{*path}` remains a compatibility alias.
- `/rest/rpc/{*path}` — REST continuation channel (e.g. large asset streams), keyed by a `Guid`.
- `GET /icons/{*path}` — a package or dependency icon from the content-addressed store. Access follows the visibility of packages referencing the bytes. Public references permit immutable public caching; private-only references require an authorized request signature and return `private, no-store`. Package versions refer to an icon by this path, `blake3/<hash>.<ext>`; `DeviceInfoMiddleware` inlines it as a data URL for StartOS before 0.4.0.3.

## RPC API surface (`registry_api`)

Subcommands (same module), each available over RPC and to the `start-registry` CLI via `with_call_remote`:

| Command   | Module                | Purpose                                                                  |
| --------- | --------------------- | ------------------------------------------------------------------------ |
| `index`   | `mod::get_full_index` | full combined index (name, icon, packages, OS, signers)                  |
| `info`    | `registry/info.rs`    | set/get registry info and categories                                     |
| `os`      | `registry/os/`        | OS version index + asset (image) management                              |
| `package` | `registry/package/`   | add/get/list packages, versions, assets; manage private-package access   |
| `admin`   | `registry/admin.rs`   | manage admins and signers                                                |
| `db`      | `registry/db.rs`      | dump/inspect the registry database; subscribe to `/index` or any subpath |
| `metrics` | `registry/metrics.rs` | download/user metrics summaries (admin-only)                             |

### Registry index subscriptions

The public `db.subscribe` RPC accepts optional `pointer` and `osVersion` parameters. The pointer
may select `/index` or any descendant; omitting it or passing `null` selects `/index`. Without
`rpcstream` opt-in, the response contains an initial `{ id, value }` dump and a continuation
`guid`. Connecting to `/rpc/v2/<guid>` streams revisions relative to the selected subtree. This
contract also applies to ordinary calls on the shared WebSocket.

With `rpcstream: "1.0"` on `/rpc/v2`, the same method returns the dump directly as its initiating
result and sends subsequent `Revision` events under that request ID. Closing a stream releases
its subscription; ordinary calls and other subscriptions retain the connection.

StartOS opens one `/rpc/v2` socket per registry, multiplexing `/index` and the selected OS source's
`/index/os` subscriptions. StartOS's account developer key authenticates its client identity during
signed-DH admission; initiating messages include the client's OS version inside that encrypted channel.
Responses permit 64 MiB logical payloads independently of request limits; wire allowances include
framing and ciphertext overhead.
Package queries and registry information use the backend cache; browser sessions observe projected
package and OS metadata through PatchDB. Subscriptions reconnect with jittered backoff and a fresh
dump. Cached metadata remains available offline, with connection availability reported separately
from registry configuration. Malformed saved source URLs are skipped independently without
rewriting configured intent. Streaming requires signed-DH admission, `rpcstream` and a configured
hostname covering the client's selected URL host. A legacy HTTP 200 without upgrading, or upgrade HTTP 404/405/501, selects signed HTTP
polling every 15 minutes, using full/all-revision `package.get`, `info` and the selected `os.index`.
Each cycle rechecks upgrade support. Successful polling marks observed availability; failed polls
retain caches without granting trust. Redirects, authorization errors and failed encrypted admission
never select fallback. Older clients can still call `db.subscribe` without opting in.

Both transports read filtered in-memory replicas and use bounded, 32-revision JSON queues. Slow
subscribers close rather than skip patches. Replicas are created lazily for each effective reader
and wire-compatibility profile. Equivalent OS versions share a profile; unversioned callers retain
the canonical index format. Public-only readers share the profile's public replica; admins and
signers with private-package access use their verified request key. Every source revision rechecks
enrollment, admin status and package permissions before applying the version-specific projection.
`device_info::WireCompatibility` owns these projections for ordinary RPC responses and replicas,
including legacy icons, notes and key/signature encodings. Canonical storage remains migrated to
the current schema; PatchDB derives patches from the projected values. A projection failure ends
its subscriptions and permits rebuilding the failed replica without closing unrelated streams.
A public-mirror subscriber must resubscribe after receiving a private-access grant. StartOS's
registry-management More menu offers refresh in every connection state, restarting that
registry's grouped watches without changing signing-key trust or discarding cached listings. Weak mirror entries share work across subscribers; dropping the last subscriber cancels that mirror's task.

StartOS derives security notifications from the selected source's OS metadata, filtering by source
version, platform, and installed build. Delivery history remains keyed by registry URL, version, and
build hash across reconnects and restarts.

## Data model

`RegistryDatabase` (in `registry/mod.rs`) is a PatchDB document:

- `migrations` — applied migration names (`registry/migrations`).
- `admins` — set of admin `Guid`s.
- `index: FullIndex` — `{ name, icon, description, package: PackageIndex, os: OsIndex, signers: BTreeMap<Guid, SignerInfo>, trustSigners: Set<Guid> }`.

The admin-only RPC `registry.info.set-trust-signers` accepts `{ signers: Guid[] }`,
selecting a subset of the existing signer records. `registry.info` returns the
selected IDs and complete records as `trustSigners: Record<Guid, SignerInfo>`.
Signer records retain their original GPG exports and per-key identities and
fingerprints. An empty selection proposes no keys. StartOS trusts newly offered
keys for packages when connecting; keys discovered by index synchronization start disabled,
with notifications for newly offered or dropped keys. StartOS trusts a registry's keys for OS updates automatically only when a user
chooses it for StartOS updates.

Package and OS indexes (`registry/package/index.rs`, `registry/os/index.rs`) hold versions, dependency `VersionRange`s, and `RegistryAsset`s. Package assets carry Merkle archive commitments; OS assets carry Blake3 commitments.

Package and OS asset ingestion accept a default-empty `signatures` map alongside the authenticated uploader's `signature`. Each submitted signature is verified against the asset commitment and signature context before merging with existing signatures. The fresh uploader signature takes precedence over a submitted signature for the same key; stored signatures remain unchanged. Promotion forwards every source mirror URL and the source signature map, and adds the promoter's signature. Endorsements do not register signers, change trust proposals, or grant publishing authorization; the authenticated uploader must retain permission to publish. Deploy the updated registry before the promoting CLI: older registries ignore the additional map.

`PackageInfo` flattens `PackageAccessControl` into its wire record: `private` defaults to false and
`readers` defaults to an empty signer-ID set. The flattened policy deserializes independently of
version metadata, including existing rows without either field. One visibility primitive admits
public packages, admins, authorized publishers, and listed readers. Browsing endpoints, mirrors,
and stored-icon references use it. Browsing and subscription projections redact reader lists for
non-admins. Admin-only `package.access` commands manage the policy. Invalid resource proofs
retain anonymous access to public icons; inaccessible private-only icons return 404.
Signed public browsing retains the verified identity without adding admin audit records.

OS versions use the shared `LocaleString` for `releaseNotes`, as package metadata does:
a plain string or a locale-to-Markdown map. The compiler includes `en_US` in translated
maps. Current clients receive the complete map and select locally with `LocalizePipe`.
Existing English-only records deserialize unchanged without a migration.

`os.version.add` replaces the entire `LocaleString`; promotion preserves it unchanged.
The existing third positional CLI argument accepts prose or a JSON locale map. The
`<RELEASE_NOTES_OR_JSON>` help marker lets publishers reject stale CLIs before writes.
Registries must be upgraded before publishing maps.

`DeviceInfoMiddleware` projects English notes in responses for StartOS before 0.4.0.5
and string-only StartWRT clients. It uses the device version, or an explicit update
source version without a device header. Headerless queries without a source version
retain the full map. Projection covers OS version queries, the OS index and the full
index; stored notes remain unchanged.

## Persistence

`RegistryContext::init` resolves `datadir` (default `/var/lib/startos`) and:

- Opens `<datadir>/registry.db` as a `TypedPatchDb<RegistryDatabase>`; initializes it on first run and runs pending migrations.
- Stores package and dependency icons once each under `<datadir>/icons/blake3/`, named by the blake3 of their bytes, and deletes those the index no longer refers to.
- Opens a SQLite connection for metrics.
- Hosts asset files under the data directory.

## Frontend

The registry has no bundled UI of its own; the browsing/search/download UI is the shared Angular library **`@start9labs/marketplace`** at `shared-libs/ts-modules/marketplace/`. App projects (StartOS web, etc.) consume that library and point it at a registry's RPC endpoints. The library is source-consumed via tsconfig paths within the `shared-libs/ts-modules` workspace of shared TypeScript modules.

## Further reading

- [README.md](./README.md) — what the registry is and how to run it.
- [AGENTS.md](./AGENTS.md) — build, test, and contribution workflow.
