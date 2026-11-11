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
- `src/net/static_server.rs` — UI and authenticated archive/icon resources. Resource signatures reuse `SignatureAuthContext::check_pubkey` for enrollment. The registry-icon proxy signs exact content-addressed icon URLs under saved registries with the server developer key, using the shared icon path validator and canonical registry enumeration. Signed upstream GETs refuse redirects; other images and archive download hosts retain their existing transport policy. Upstream status, content type and cache policy pass through; unrelated response headers do not.
- `src/s9pk/` — S9PK package format (merkle archive)
- `src/lxc/` — LXC container management for packages
- `src/install/`, `src/update/` — Package install and OS/package update flows
- `src/trusted_keys.rs` — Individual signing-key trust (`TrustedKey.publicKey`), immutable import provenance, committed initial certificates in `src/trusted_keys/initial/`, and legacy OS-signer adapters. `src/trusted_keys/legacy.rs` owns historical grouped-keyring decoding and migration to canonical per-key records; downgrade restores the historical array shape. `src/trusted_keys/connection.rs` atomically records registry connections and advertised-key snapshots. Connecting trusts keys new to the server for packages. Registry index synchronization imports newly listed keys disabled, preserves existing trust and provenance, and issues a `RegistryKeyAdded` notification for each key new to the server and a `RegistryKeyDropped` notification for each key the registry stops offering. Selecting the StartOS update registry trusts its listed keys for OS updates, and that registry cannot be removed. Connected registry listings determine deletion protection, and the last OS-update key cannot lose its trust. `src/sign/gpg.rs` retains original exports and attributes self-certified identities and OpenPGP fingerprints to each signing key.
- `src/update/signer_files.rs` — Mirrors OS-update trust to PEM files in the config overlay at `/etc/startos/os-signers`, reconciled from the database during RPC initialization after database migrations. Trust-changing RPCs serialize database mutations and await filesystem persistence. Diagnostic updates read that selection without the data drive, falling back to the image's three production keys only before a selection has been saved. Regular `LocalAuth` verifies its protected cookie, strips caller-supplied `__Auth_local` claims, and injects its authentication result when `get_local_auth` metadata requests it. The diagnostic update endpoint uses that injected result to authorize trust overrides.
- `src/registry/sync.rs` — StartOS-owned registry connections, reconnects, and cache projections. One multiplexed socket per registry carries `/index` and, for the selected OS source, `/index/os` watches. `sync/connection.rs` authenticates a signed-DH session with `AccountInfo.developer_key` before sending initiating JSON; each subscription includes the client's OS version inside that encrypted channel. It decodes ordinary and streamed replies through rpc-toolkit's shared response types. Its 64 MiB logical response budget is independent of incoming RPC request limits; wire allowances include framing and ciphertext overhead. Private raw indexes serve backend package selection through the registry's shared selector; public `registryCache` and `osUpdateCache` expose derived metadata and observed connection availability. Configuration remains in `public.ui`. The RPC context owns these tasks, which release their strong context before receiving updates. Its bounded, ephemeral refresh queue restarts only the requested registry's group, retaining caches and awaiting the old task before replacement. `server.trusted-keys.refresh-registry` queues resubscription without a separate discovery fetch; snapshot publication reuses disabled-key discovery without granting trust. Connecting retains its distinct initial-trust policy. The marketplace offers refresh for saved registries and explicit connection for unsaved URLs. Failed icon loads leave the shared cache, allowing refreshed listings to retry. Uncached backend queries and initial registry-info discovery share `registry/context.rs::call`, the existing SOCKS-aware upgrade and `session::client` admission/request primitives. Device-info headers survive upgrade. HTTP 404/405/501 on the upgrade route selects signed legacy HTTP for discovery and uncached queries; redirects and rejected encrypted admission never trigger that fallback. The existing registry task polls `package.get` with full version details, registry `info` and the selected `os.index` every 15 minutes, replacing its cached index through the same projection/trust owner. Each polling cycle retries the upgrade, allowing an upgraded registry to resume streaming. Refresh/removal cancel the same group and in-flight work. Successful polling marks observed availability without implying a persistent socket. Legacy RPC uses the selected host for signatures, the existing SOCKS client without redirects, a 30-second request deadline and a 64 MiB JSON reply limit. Ordinary asset clients retain their redirect behavior.
- `src/registry/` — Package registry server and management. `SignerInfo` retains canonical keys and full OpenPGP exports. `FullIndex.trust_signers` selects signer IDs; `registry.info.trustSigners` projects those existing signer records into the public trust proposal.
- `src/tunnel/` — StartTunnel server logic
- `src/backup/`, `src/sign/` — Backup and signing. `sign/session.rs` authenticates two-message ephemeral-DH exchanges through the existing generic request-commitment proof owner and standard X25519/HKDF/AES-GCM implementations. Signed logical targets bind RPC and individual GUID intent; the existing signature nonce owner remains shared. `sign/fingerprint.rs` owns comparable identity fingerprints. `sign/instance.rs` loads the registry, Tunnel and WRT instance's private `rpc-identity.pem`; first creation publishes a complete file without replacing a concurrent winner, preserves malformed files for recovery, and enforces mode `0600`. StartOS retains its existing account developer identity.
- `src/os_install/`, `src/init.rs`, `src/setup.rs` — OS install, init, and first-run setup
- `src/util/` — Shared utilities (process invocation, IO, guards — see `core-rust-patterns.md`). `util/net.rs::WebSocket` retains one generic message-transport lifecycle for keepalive, send deadlines and close handling. `util/net/secure.rs::EncryptedSocket` adds signed-DH admission, typed encrypted data/close-result framing and independent logical/wire limits without a second socket manager. Authenticated close results retain caller-visible status/reasons; physical controls carry no application close result. `MessageKind` owns the wire-kind mapping; see [Encrypted RPC integration](encrypted-rpc.md) and the [signed-session wire contract](signed-session.md).
- `src/version/` — Migrations and version logic (see `VERSION_BUMP.md`, `exver.md`)

## DNS lifecycle

`startd` creates one shared `DnsController` before selecting setup, initialization, or diagnostic UI. Those contexts and the runtime `NetController` share its handle and listener-reconciliation task. Attaching the persisted `NetworkInterfaceController` transfers DNS authority and keeps that controller alive independently of later RPC initialization success. See [DNS forwarding](policy-routing.md#dns-forwarding) for bootstrap policy, watcher handoff, and requester-based routing.

## RPC Pattern

The control API uses JSON-RPC. RPC methods are organized in a hierarchical command structure using [rpc-toolkit](https://github.com/Start9Labs/start-technologies/tree/master/shared-libs/crates/rpc-toolkit). Handlers are registered in a tree of `ParentHandler` nodes, with four handler types: `from_fn_async` (standard), `from_fn_async_local` (non-Send), `from_fn` (sync), and `from_fn_blocking` (blocking). Metadata like `.with_about()` drives middleware and documentation.

`SigningContext::rpc_signer` owns native key/local-token precedence and absent-key policy.
`CliContext::call_rpc` and WRT's CLI own `session::client::SessionClient`, which lazily
establishes and reuses connections scoped by endpoint, selected name, signer and headers.
The message-based `util::rpc_client::RpcClient` dispatches both Unix-line and encrypted
WebSocket responses; its idle reader drives keepalive. Transport failure never replays a
possibly sent request, and application errors retain healthy sockets. Startup CLI
contexts and `state` select HTTP explicitly. `CliWebSocketContext` selects continuation
admission from the originating context, without per-GUID/global-key security state.
Configured network policy and the native message conversion are shared; result consumers
require application completion rather than physical stream loss.

GUID WebSocket continuations carry explicit `WebSocketPolicy` and optional `SecureSession` provenance from their issuing request. Anonymously available streams use bounded input and write deadlines; authenticated container, terminal and runtime streams retain bulk input. Secure-origin GUIDs require a fresh signed-DH exchange with the same initiating key, server-name context and exact signed GUID intent before callback execution; legacy-origin GUIDs remain plaintext-compatible. `WebSocketFuture` owns revocation across admission and callback. Resource policy, revocation and encryption provenance are independent.

`middleware/auth/session.rs` requires encrypted admission on bare `/rpc/v2`, reusing `request_identities` for allowed ingress/loopback identities. The signed CBOR body carries protocol/version, not URL selectors; each hello appends its structured binary commitment proof. `SignatureAuthContext` supplies identity, replay and scoped-session installation capabilities. Registry registers the upgrader on `/rpc/v2`; the native registry client uses the shared Tungstenite transport bridge and the existing SOCKS-aware client under its connection deadline. Successful exchange installs immutable peer/name/server-key provenance and a weak live replay capability on a context clone, not its shared seed. RPC and GUID admission share `RequestReplayCheck` over the existing nonce/clock policy, acquiring context only when polled. `SignatureAuth` derives the signer from that scope and still checks live enrollment, permissions and post-auth hooks for every RPC; caller proofs cannot replace an encrypted connection's identity. Product routes register the upgrader explicitly. Setup/init/diagnostic contexts have no session scope, and HTTP resources retain their independent policy.

`middleware/cors.rs` owns the shared `tower-http` CORS layer. StartOS, registry and Tunnel apply it at their HTTP RPC routes, before body limits, context construction and authentication; StartOS also covers MCP. WRT retains its daemon-wide scope and exposed firmware hash. The platform owns mirrored preflight permissions and `Vary` handling. UI resource routes retain their independent policies.

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
- [encrypted-rpc.md](encrypted-rpc.md) — Encrypted transport integration, ownership and compatibility
- [signed-session.md](signed-session.md) — Signed ephemeral-DH admission and record wire contract
- [patchdb.md](patchdb.md) — Patch-DB watch patterns and TypedDbWatch
- [i18n-patterns.md](i18n-patterns.md) — Internationalization conventions
- [core-rust-patterns.md](core-rust-patterns.md) — Common Rust utilities
- [s9pk-structure.md](s9pk-structure.md) — S9PK package format
- [policy-routing.md](policy-routing.md) — the `ip rule` ladder: its invariants, order, and history
