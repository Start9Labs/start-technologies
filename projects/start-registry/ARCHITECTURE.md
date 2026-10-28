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

- `POST /rpc/{*path}` — JSON-RPC, wrapped in `Cors`, `Auth` (local + signature auth), and `DeviceInfoMiddleware`.
- `GET /ws/rpc/{*path}` — WebSocket RPC continuations, keyed by a `Guid`.
- `/rest/rpc/{*path}` — REST continuation channel (e.g. large asset streams), keyed by a `Guid`.
- `GET /icons/{*path}` — a package or dependency icon from the content-addressed store, unauthenticated and immutable. Package versions refer to an icon by this path, `blake3/<hash>.<ext>`; `DeviceInfoMiddleware` inlines it as a data URL for StartOS before 0.4.0.3.

## RPC API surface (`registry_api`)

Subcommands (same module), each available over RPC and to the `start-registry` CLI via `with_call_remote`:

| Command   | Module                | Purpose                                                                  |
| --------- | --------------------- | ------------------------------------------------------------------------ |
| `index`   | `mod::get_full_index` | full combined index (name, icon, packages, OS, signers)                  |
| `info`    | `registry/info.rs`    | set/get registry info and categories                                     |
| `os`      | `registry/os/`        | OS version index + asset (image) management                              |
| `package` | `registry/package/`   | add/get/list packages, versions, assets                                  |
| `admin`   | `registry/admin.rs`   | manage admins and signers                                                |
| `db`      | `registry/db.rs`      | dump/inspect the registry database; subscribe to `/index` or any subpath |
| `metrics` | `registry/metrics.rs` | download/user metrics summaries (admin-only)                             |

### Registry index subscriptions

The public `db.subscribe` RPC accepts `{ "pointer": <JSON Pointer> }`. The pointer may select
`/index` or any descendant; omitting it or passing `null` selects `/index`. The response contains
an initial `{ id, value }` dump and a continuation `guid`. Connecting to `/ws/rpc/<guid>` streams
revisions whose JSON Patch paths are relative to the selected subtree.

## Data model

`RegistryDatabase` (in `registry/mod.rs`) is a PatchDB document:

- `migrations` — applied migration names (`registry/migrations`).
- `admins` — set of admin `Guid`s.
- `index: FullIndex` — `{ name, icon, package: PackageIndex, os: OsIndex, signers: BTreeMap<Guid, SignerInfo> }`.

Package and OS indexes (`registry/package/index.rs`, `registry/os/index.rs`) hold versions, dependency `VersionRange`s, and `RegistryAsset`s. Assets carry **Blake3 commitments** (`sign/commitment/blake3`) for content addressing and integrity, and are signed by registered signers.

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
