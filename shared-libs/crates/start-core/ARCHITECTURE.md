# start-core Architecture

The shared Rust backend library for StartOS. Cargo package `start-core`, library name `start_core`,
rooted at `shared-libs/crates/start-core`. All Start9 product binaries link against it.

## Library, not a binary

This crate is a **library** (`src/lib.rs`). It exposes every backend subsystem plus a set of
entrypoints under `src/bins/`. The product crates are thin wrappers that pick which entrypoints
to enable via `start_core::bins::MultiExecutable` and call `.execute()`:

| Binary                | Wrapper crate / file                           | Role                                                                            |
| --------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------- |
| `startbox` / `startd` | `projects/start-os/src/bin/startbox.rs`        | Main OS daemon                                                                  |
| `start-container`     | `projects/start-os/src/bin/start-container.rs` | Runs inside package LXC containers; talks to the host and manages subcontainers |
| `start-cli`           | `projects/start-cli/src/main.rs`               | CLI over the daemon's JSON-RPC API                                              |
| `registrybox`         | `projects/start-registry/src/main.rs`          | Package registry server                                                         |
| `tunnelbox`           | `projects/start-tunnel/src/main.rs`            | StartTunnel VPN/forwarding server                                               |

A sixth bin, `startwrt` (`projects/start-wrt/backend/ctrl/src/bin/startwrt.rs`), also links
against this crate but is not a `MultiExecutable` wrapper — it is a full backend of its own
that imports the crate aliased as `startos` for its `net`/`util` primitives.

`MultiExecutable` also supports invoking a chosen entrypoint by argv[0] (busybox-style), which is
how `startbox` dispatches to `startd`, `start-cli`, etc. The per-entrypoint logic lives in
`src/bins/{startd.rs, start_cli.rs, container_cli.rs, registry.rs, tunnel.rs, start_init.rs, …}`.

## Key Modules

- `src/bins/` — Per-binary entrypoints + `MultiExecutable` dispatch
- `src/util/output.rs` — Table output under `rpc_toolkit::output`'s broken-pipe policy; the prelude re-exports it.
- `src/context/` — Context types (RpcContext, CliContext, InitContext, DiagnosticContext)
- `src/service/` — Service lifecycle management with actor pattern (`service_actor.rs`)
- `src/db/model/` — Patch-DB models (`public.rs` synced to frontend, `private.rs` backend-only)
- `src/net/` — Networking (DNS, ACME, WiFi, Tor, WireGuard, gateway/NAT)
- `src/s9pk/` — S9PK package format (merkle archive)
- `src/lxc/` — LXC container management for packages
- `src/install/`, `src/update/` — Package install and OS/package update flows
- `src/trusted_keys.rs` — Individual signing-key trust (`TrustedKey.publicKey`), immutable import provenance, committed initial certificates in `src/trusted_keys/initial/`, and legacy OS-signer adapters. `src/trusted_keys/legacy.rs` owns historical grouped-keyring decoding and migration to canonical per-key records; downgrade restores the historical array shape. `src/trusted_keys/connection.rs` atomically records registry connections and advertised-key snapshots. Connecting trusts keys new to the server for packages. Proxied package and OS-version requests to a connected registry start a rate-limited check that imports newly listed keys disabled, preserves existing trust and provenance, and issues a `RegistryKeyAdded` notification for each key new to the server and a `RegistryKeyDropped` notification for each key the registry stops offering. Selecting the StartOS update registry trusts its listed keys for OS updates, and that registry cannot be removed. Connected registry listings determine deletion protection, and the last OS-update key cannot lose its trust. `src/sign/gpg.rs` retains original exports and attributes self-certified identities and OpenPGP fingerprints to each signing key.
- `src/update/signer_files.rs` — Mirrors OS-update trust to PEM files in the config overlay at `/etc/startos/os-signers`, reconciled from the database during RPC initialization after database migrations. Trust-changing RPCs serialize database mutations and await filesystem persistence. Diagnostic updates read that selection without the data drive, falling back to the image's three production keys only before a selection has been saved. Regular `LocalAuth` verifies its protected cookie, strips caller-supplied `__Auth_local` claims, and injects its authentication result when `get_local_auth` metadata requests it. The diagnostic update endpoint uses that injected result to authorize trust overrides.
- `src/registry/` — Package registry server and management. `SignerInfo` retains canonical keys and full OpenPGP exports. `FullIndex.trust_signers` selects signer IDs; `registry.info.trustSigners` projects those existing signer records into the public trust proposal.
- `src/tunnel/` — StartTunnel server logic
- `src/backup/`, `src/sign/` — Backup and signing
- `src/os_install/`, `src/init.rs`, `src/setup.rs` — OS install, init, and first-run setup
- `src/util/` — Shared utilities (process invocation, IO, guards — see `core-rust-patterns.md`)
- `src/version/` — Migrations and version logic (see `VERSION_BUMP.md`, `exver.md`)

## DNS lifecycle

`startd` creates one shared `DnsController` before selecting setup, initialization, or diagnostic UI. Those contexts and the runtime `NetController` share its handle and listener-reconciliation task. Attaching the persisted `NetworkInterfaceController` transfers DNS authority and keeps that controller alive independently of later RPC initialization success. See [DNS forwarding](policy-routing.md#dns-forwarding) for bootstrap policy, watcher handoff, and requester-based routing.

## RPC Pattern

The control API uses JSON-RPC. RPC methods are organized in a hierarchical command structure using [rpc-toolkit](https://github.com/Start9Labs/start-technologies/tree/master/shared-libs/crates/rpc-toolkit). Handlers are registered in a tree of `ParentHandler` nodes, with four handler types: `from_fn_async` (standard), `from_fn_async_local` (non-Send), `from_fn` (sync), and `from_fn_blocking` (blocking). Metadata like `.with_about()` drives middleware and documentation.

See [rpc-toolkit.md](rpc-toolkit.md) for full handler patterns and configuration.

## Action file transfers

`ServiceSeed.action_files` owns transient prepared descriptors and completed upload receipts; `RpcContinuations` owns their single-use HTTP endpoints. The store scopes IDs to the service and procedure event, with expiry independent of SDK form caching. Successful uploads remain service-owned. Returning a File result arms its requested download cleanup after action/task processing succeeds. Service teardown cancels active transfers.

Volume confinement uses retained directory descriptors and no-follow traversal. Packages must coordinate their writers and keep prepared paths stable until transfer and cleanup finish.

## Patch-DB Patterns

Patch-DB provides diff-based state synchronization. Changes to `db/model/public.rs` automatically sync to the frontend.

**Key patterns:**

- `db.peek().await` — Get a read-only snapshot of the database state
- `db.mutate(|db| { ... }).await` — Apply mutations atomically, returns `MutateResult`
- `#[derive(HasModel)]` — Derive macro for types stored in the database, generates typed accessors

**Generated accessor types** (from `HasModel` derive):

- `as_field()` — Immutable reference: `&Model<T>`
- `as_field_mut()` — Mutable reference: `&mut Model<T>`
- `into_field()` — Owned value: `Model<T>`

**`Model<T>` APIs** (from `db/prelude.rs`):

- `.de()` — Deserialize to `T`
- `.ser(&value)` — Serialize from `T`
- `.mutate(|v| ...)` — Deserialize, mutate, reserialize
- For maps: `.keys()`, `.as_idx(&key)`, `.as_idx_mut(&key)`, `.insert()`, `.remove()`, `.contains_key()`

See [patchdb.md](patchdb.md) for `TypedDbWatch<T>` construction, API, and usage patterns.

## Backup conversion

`backup.target.convert` authenticates the selected target/server backup's existing credential and starts a background conversion. `PreparedConversion` owns the mount, data path and zeroizing credential; the worker retains only the database and shutdown receiver, not the full RPC context. The filesystem helper takes its existing exclusive store lock. Server shutdown cancels the helper before releasing the mount. Backup acceptance shares the per-target/server running-conversion check before publishing progress; the helper lock still arbitrates races, reversed starts and externally launched operations. Conversion reuses the authenticated pending marker and durable migration pointers on the backup, rather than trusting progress counters.

`ServerStatus.backupConversions` holds observed jobs keyed by the target's canonical string and server ID. `FullProgressTracker` streams phase/count updates through Patch-DB; the existing initialization reset clears these transient observations on restart. The helper's `conversion.pending` discovery hint lets another session or server offer explicit resumption. That hint and `StartOsRecoveryInfo.formatStatus` are unauthenticated observations, not permission to mutate a store.

`backup-fs-protocol` owns the portable helper messages shared by core and the Linux filesystem implementation. Core uses `Invoke::output_to` to consume bounded JSON progress lines and preserves stdin credential transport. The helper throttles progress output and aggregates corruption diagnostics; neither channel contains credentials.

## Registry asset transports

`registry::asset` owns signed asset commitments and source selection. Optional `torrent` metadata advertises an external magnet link or HTTP(S) torrent URL alongside HTTP mirrors. Registries store the metadata; operators seed the files elsewhere.

`registry::bittorrent` uses librqbit for single-file transfers, bounds metadata and no-progress waits, and verifies complete downloads through the existing commitment implementations before payload handoff. Failed torrent attempts use the existing HTTP sources and mirror retries. Registry package installs race authenticated metadata from advertised HTTP mirrors and librqbit's verified-piece stream. The native workflow registers `Installing`/`Updating` state and cancellation before awaiting full payload completion; progress synchronization starts before that wait. Ready-source sideload/restore callers reuse the same native workflow. The final source is independently parsed and checked against the caller's original commitment before filtering/unpacking, avoiding byte-offset reuse across equivalent archive serializations. Shared preparation/completion futures retain one transfer and its metadata for both installation and post-success registration; cancellation closes the payload session even if metadata readers survive. Other torrent consumers still receive fully buffered sources, while HTTP downloads retain streaming reads. Diagnostic OS updates stream over HTTP directly to the image partition; `/var/tmp` can remain RAM-backed before data-disk initialization.

Each transient transfer owns a cancellable engine session. CLI sessions disable uploads and write no sharing catalog. StartOS persists cascading `downloads`, `seeding` and `incoming` intent in `ServerInfo.bittorrent`, defaulting to downloads only. Disabling downloads cancels active peer transfers for HTTP fallback; disabling either parent clears its children. YAML configures torrent-metadata storage and an administrator transport cap, not listener activation. Incoming connections reuse the OS `NetService`, admin host, native port allocation, address-scoped plaintext proxy and PCP/UPnP forwarding. Its `ProxyOptions::Tcp` relays raw streams to librqbit on loopback; the engine announces the assigned external port with its independent UPnP disabled. HTTP proxy policy keeps its existing path and SDK helper contract. The Software sharing page uses the shared interface/address-requirements UI and the authoritative default-outbound gateway state. Completed installs return the authoritative retained archive path; OS updates register the image selected by the native upgrade pointer. Registration stores descriptors, not payload copies. The existing context-owned cron watches intent, checks registered sources every 30 seconds, and discovers advertisements for pre-existing installed packages and retained OS images every five minutes. Discovery browses each package's full version catalog without a targeted version, preserving installed revisions without incrementing package-request metrics. Install and discovery queries share the typed registry transport and asset selection; OS discovery uses the public index and both use the bounded metainfo resolver. A weak context provider avoids a context/task reference cycle; metadata requests and local reconciliation share the task without blocking each other. A read-only storage adapter and librqbit's paused piece check reject incompatible files without repair; installed architecture filtering or trailer removal can make published torrent bytes incompatible. Serving reopens sources by path, while installation, uninstall and image pruning retain ownership of their lifetimes. Signed verification uses the same source through bounded 256-KiB reads, releasing file descriptors between chunks rather than pinning an entire image during verification. Removed sources lose their catalog entries and sessions. Source and metadata fingerprints avoid repeated whole-file checks when neither changes. Catalog updates and cleanup share per-record `FileLock`s; unchanged registration preserves the metadata file and avoids redundant payload revalidation. A process lease protects the metadata directory. Shutdown awaits cron cancellation before stopping the engine. Onion registries and proxied CLI downloads exclude peer transport.

## Notification delivery

`notifications::notify` persists notifications and enqueues push deliveries in the same transaction. `notifications::push` owns authenticated browser subscriptions, the server's VAPID identity, encrypted request construction, and the bounded private outbox. Its sender runs under `RpcContext`'s existing cron ownership, outside database mutations, and uses the reloadable SOCKS-aware HTTP client. Push transport failures do not prevent local notification creation.

The StartOS frontend uses Angular `SwPush` and the stock Angular service worker. Browser subscriptions persist consent; authenticated portal startup and subscription-change events synchronize the server's delivery targets. Browser-provided HTTP(S) endpoints are trusted administrator input, including LAN addresses.

## i18n

See [i18n-patterns.md](i18n-patterns.md) for internationalization key conventions and the `t!()` macro.

## Rust Utilities & Patterns

See [core-rust-patterns.md](core-rust-patterns.md) for common utilities (Invoke trait, Guard pattern, mount guards, Apply trait, etc.).

## Cross-layer verification

Rust structs derive `visit_rs::VisitFields` and enums derive
`visit_rs::VisitVariants`, supplying raw facts and static traversal; RPC owns direction-aware serde
normalization and TypeScript rendering through adjacent `rpc_toolkit::reflect_ts!`
bridges. `rpc_toolkit::ts_export!` registrations beside those types own standalone
namespace roots. Raw `visit(ts(...))` hints remain opaque to reflection. Local
`type_attributes` selectors compile authoritative wire-hint/conversion literals in
the owner scope; explicit `visit(opaque)` avoids hidden storage bounds. Actual
decoder DTOs own hook fidelity, and typed `TS::IS_OPTION` combines with serde
policy rather than syntactic field tests. The generated contracts serve the web UI and the package
container-runtime through the SDK. They do **not** propagate automatically. From the repo root:

1. `make start-core-ts-bindings` — regenerates `shared-libs/crates/start-core/bindings/` (via `build/build-ts.sh`),
   then rsyncs it into `shared-libs/ts-modules/start-core/lib/osBindings/`.
2. `cd shared-libs/ts-modules/start-core && make dist` (what web imports) and
   `cd projects/start-sdk && make bundle` (the SDK bundle container-runtime imports) — rebuilds the
   TS bundles that downstream actually imports.

Until both steps run, a changed `ts_export!` type is out of sync with everything downstream.

## Further reading

- [README.md](README.md) — what this crate is and how to use it
- [AGENTS.md](AGENTS.md) — build, test, format, and operating rules (`CLAUDE.md` is a one-line `@AGENTS.md` import)
- [rpc-toolkit.md](rpc-toolkit.md) — JSON-RPC handler patterns
- [patchdb.md](patchdb.md) — Patch-DB watch patterns and TypedDbWatch
- [i18n-patterns.md](i18n-patterns.md) — Internationalization conventions
- [core-rust-patterns.md](core-rust-patterns.md) — Common Rust utilities
- [s9pk-structure.md](s9pk-structure.md) — S9PK package format
- [policy-routing.md](policy-routing.md) — the `ip rule` ladder: its invariants, order, and history
