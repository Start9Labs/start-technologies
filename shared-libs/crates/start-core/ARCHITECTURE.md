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
- `src/net/static_server.rs` — UI and authenticated archive/icon resources. Resource signatures reuse `SignatureAuthorizationContext::check_pubkey` for enrollment. The registry-icon proxy signs exact content-addressed icon URLs under saved registries with the server identity key, using the shared icon path validator and canonical registry enumeration. Signed upstream GETs refuse redirects; other images and archive download hosts retain their existing transport policy. Upstream status, content type and cache policy pass through; unrelated response headers do not.
- `src/s9pk/` — S9PK package format (merkle archive)
- `src/lxc/` — LXC container management for packages
- `src/install/`, `src/update/` — Package install and OS/package update flows
- `src/trusted_keys.rs` — Individual signing-key trust (`TrustedKey.publicKey`), immutable import provenance, committed initial certificates in `src/trusted_keys/initial/`, and legacy OS-signer adapters. `src/trusted_keys/legacy.rs` owns historical grouped-keyring decoding and migration to canonical per-key records; downgrade restores the historical array shape. `src/trusted_keys/connection.rs` atomically records registry connections and advertised-key snapshots. Connecting trusts keys new to the server for packages. Registry index synchronization imports newly listed keys disabled, preserves existing trust and provenance, and issues a `RegistryKeyAdded` notification for each key new to the server and a `RegistryKeyDropped` notification for each key the registry stops offering. Selecting the StartOS update registry trusts its listed keys for OS updates, and that registry cannot be removed. Connected registry listings determine deletion protection, and the last OS-update key cannot lose its trust. `src/sign/gpg.rs` retains original exports and attributes self-certified identities and OpenPGP fingerprints to each signing key.
- `src/update/signer_files.rs` — Mirrors OS-update trust to PEM files in the config overlay at `/etc/startos/os-signers`, reconciled from the database during RPC initialization after database migrations. Trust-changing RPCs serialize database mutations and await filesystem persistence. Diagnostic updates read that selection without the data drive, falling back to the image's three production keys only before a selection has been saved. Regular `LocalAuth` verifies its protected cookie, strips caller-supplied `__Auth_local` claims, and injects its authentication result when `get_local_auth` metadata requests it. The diagnostic update endpoint uses that injected result to authorize trust overrides.
- `src/registry/sync.rs` — StartOS-owned registry connections, reconnects, and cache projections. One multiplexed socket per registry carries `/index` and, for the selected OS source, `/index/os` watches. `sync/connection.rs` authenticates a signed-DH session with `AccountInfo.server_identity_key` before sending initiating JSON; each subscription includes the client's OS version inside that encrypted channel. It decodes ordinary and streamed replies through rpc-toolkit's shared response types. Its 64 MiB logical response budget is independent of incoming RPC request limits; wire allowances include framing and ciphertext overhead. Private raw indexes serve backend package selection through the registry's shared selector; public `registryCache` and `osUpdateCache` expose derived metadata, observed connection availability and optional failure text. A retry or successful snapshot clears failure text; missing fields in older snapshots default to absent. Connected registries and the selected OS source are owned by `public.serverInfo.registries` and `public.serverInfo.osRegistry`; UI state stays opaque outside migrations. Separate watches on those two settings exclude progress, network, notification and trust-snapshot writes from reconciliation. Both selectors are read from one database snapshot; explicit Refresh still restarts its group without requiring a configuration change. The RPC context owns these tasks, which release their strong context before receiving updates. Its bounded, ephemeral refresh queue restarts only the requested registry's group, retaining caches and awaiting the old task before replacement. `server.registries.refresh` queues resubscription without a separate discovery fetch; snapshot publication reuses disabled-key discovery without granting trust. Connecting retains its distinct initial-trust policy. Registry management's More menu offers manual refresh for every saved registry in every connection state; Unreachable-only status buttons provide retry shortcuts. The marketplace offers explicit connection for unsaved URLs. Failed icon loads leave the shared cache, allowing refreshed listings to retry. Uncached backend queries and initial registry-info discovery share `registry/context.rs::call`, the existing SOCKS-aware upgrade and `session::client` admission/request primitives. Device-info headers survive upgrade. A legacy HTTP 200 without upgrading, or HTTP 404/405/501 on the upgrade route, selects signed legacy HTTP for discovery and uncached queries; redirects and rejected encrypted admission never trigger that fallback. The existing registry task polls `package.get` with full version details, registry `info` and the selected `os.index` every 15 minutes, replacing its cached index through the same projection/trust owner. Each polling cycle retries the upgrade, allowing an upgraded registry to resume streaming. Refresh/removal cancel the same group and in-flight work. Successful polling marks observed availability without implying a persistent socket. Legacy RPC uses the selected host for signatures, the existing SOCKS client without redirects, a 30-second request deadline and a 64 MiB JSON reply limit. Ordinary asset clients retain their redirect behavior.
- `src/registry/` — Package registry server and management. `SignerInfo` retains canonical keys and full OpenPGP exports. `FullIndex.trust_signers` selects signer IDs; `registry.info.trustSigners` projects those existing signer records into the public trust proposal.
- `src/tunnel/` — StartTunnel server logic
- `src/backup/`, `src/sign/` — Backup and signing. `sign/session.rs` authenticates two-message ephemeral-DH exchanges through the existing generic request-commitment proof owner and standard X25519/HKDF/AES-GCM implementations. Signed logical targets bind RPC and individual GUID intent; the existing signature nonce owner remains shared. `sign/fingerprint.rs` owns comparable identity fingerprints. `sign/instance.rs` persists registry, Tunnel and WRT `serverIdentityKey` fields in their existing PatchDBs before admission, using the database's idempotent transaction owner. Missing keys generate once; malformed stored keys fail startup without rotation. StartOS retains its existing account `server_identity_key`; its persisted `developerKey` name remains compatible with existing databases.
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
WebSocket responses; its idle reader drives keepalive. Typed encrypted response streams
extend the same response-slot map with bounded item queues and reserved terminals.
Cancellation marks slots until the owned reader writes close frames concurrently with
incoming traffic, without per-call tasks. Initiating metadata and final results remain
separate; transport loss discards unfinished queues. Container/effects Unix-line callers
retain ordinary replies without a transport fallback. Transport failure never replays a
possibly sent request, and application errors retain healthy sockets. Startup CLI
contexts and `state` use a client-lifetime possession identity independently of stored
keys; diagnostic local-cookie authorization retains its own namespace. `CliWebSocketContext` selects continuation
admission from the originating context, without per-GUID/global-key security state.
Configured network policy and the native message conversion are shared; result consumers
require application completion rather than physical stream loss.

GUID WebSocket continuations carry explicit `WebSocketPolicy` and optional `SecureSession` provenance from their issuing request. Anonymously available streams use bounded input and write deadlines; authenticated container, terminal and runtime streams retain bulk input. Secure-origin GUIDs require a fresh signed-DH exchange with the same initiating key, server-name context and exact signed GUID intent before callback execution; legacy-origin GUIDs remain plaintext-compatible. `WebSocketFuture` owns revocation across admission and callback. Resource policy, revocation and encryption provenance are independent.

`middleware/auth/session.rs` requires encrypted admission on bare `/rpc/v2`, reusing `request_identities` for allowed ingress/loopback identities. The signed CBOR body carries protocol/version, not URL selectors; each hello appends its structured binary commitment proof. `SignatureAuthContext` supplies identity, replay and scoped-session installation capabilities. Registry registers the upgrader on `/rpc/v2`; the native registry client uses the shared Tungstenite transport bridge and the existing SOCKS-aware client under its connection deadline. Successful exchange installs immutable peer/name/server-key provenance and a weak live replay capability on a context clone, not its shared seed. RPC and GUID admission share `RequestReplayCheck` over the existing nonce/clock policy, acquiring context only when polled. `SignatureAuth` derives the signer from that scope and still checks live enrollment, permissions and post-auth hooks for every RPC; caller proofs cannot replace an encrypted connection's identity. Product routes register the upgrader explicitly. Setup/init/diagnostic seeds own ephemeral identities and bounded nonce state; cloned contexts carry session provenance without an enrolled-key database. `SignatureAuthorizationContext` retains database-backed per-call authorization. Startup admission uses the existing local-host gate's approved request name. Its HTTP JSON/no-CORS boundary remains; upgrades require same-origin browser requests. `WebServer` captures its router watch epoch for upgraded reads, ending them on router replacement through `take_until`. HTTP resources retain their independent policy.

`middleware/cors.rs` owns the shared `tower-http` CORS layer. StartOS, registry and Tunnel apply it at their HTTP RPC routes, before body limits, context construction and authentication; StartOS also covers MCP. WRT retains its daemon-wide scope and exposed firmware hash. The platform owns mirrored preflight permissions and `Vary` handling. UI resource routes retain their independent policies.

See [rpc-toolkit.md](rpc-toolkit.md) for full handler patterns and configuration.

`InitContext` retains the actual RpcContext initialization result in its existing watch. Its
boot owners publish that result; firmware/reboot branches remain pending. `init.subscribe`
forwards progress until that result through typed same-ID events or the ordinary GUID
continuation. Dropped result ownership is cancellation, not completion. State probes still
establish the running daemon's availability after router replacement.

`SetupContext` reserves its worker before execution and retains the application outcome in
its existing result cell. Its synchronous task slot owns a shared non-detaching join;
observers await that task without owning or restarting it. Progress completion requires
success. Typed `setup.status` and GUID observers reuse the shared progress/result forwarding
and retain a weak context reference while waiting, including unconsumed continuations.
Ordinary replies retain GUIDs; opted-in running status returns progress with `guid: null`.
Successful observation permits the wizard's separate `setup.complete` finalization call.

## Log observation

`logs.rs::LogStream` owns the followed journal process and drains its entries before checking its actual exit result through the shared `Invoke` output validator. Existing log-follow methods return same-ID `LogEntry` items and a unit terminal when streaming is requested; ordinary replies retain a real GUID and typed replies use `guid: null`. Prepared observation preserves the first entry and cursor for both adapters. `logs/context.rs::LogContext` binds forwarding/continuation creation to existing authority owners: normal logs use `OpenAuthedContinuations` for live revocation, startup logs retain their public contexts without an enrollment store. Native follow drains items and requires the actual terminal through `CliWebSocketContext`'s existing normal/diagnostic/startup transport selection; non-follow reads remain ordinary. Stdout EOF alone does not finish the observation; dropping it kills the owned child. Snapshot reads retain their checked finite invocation. `BootIdentifier` accepts string IDs or signed 32-bit integral indices; decoding and generated numeric constraints use that type's bounds.

`system::observe_metrics` snapshots and marks the existing sensor `Watch<Option<Metrics>>`, then observes its coalesced available updates. Disposing an observation releases its clone, not the sensor producer. `server.metrics.follow` exposes that snapshot plus typed Metrics items/unit terminal or an ordinary GUID; typed and ordinary observations retain existing live revocation owners. Sensor shutdown does not promise a terminal before process replacement. Logs and metrics share `rpc_continuations::forward_json_stream` for legacy GUID payload forwarding; callers retain their result-close/logging policies. Finite native metrics reads remain ordinary.

Native `package.attach` retains its bidirectional continuation protocol. `service::attach_output` requires an explicit four-byte big-endian exit value; normal Close/EOF does not establish process success. Its existing stdin worker and bounded queue are released through a per-attach cancellation socket pair without changing stdin's flags or adding a polling timer. `util::serde::deserialize_os_strings` accepts declared JSON strings and existing native argument encodings through Serde's own decoder; CLI byte-preserving emission remains compatible.

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

## Backup targets

`Public.backupTargets` owns registered local-folder, external-folder and CIFS destinations. Runtime backup/restore/conversion methods resolve these IDs; setup retains its inline recovery-source contract. Attached drives register through source creation or explicit `backup.target.find`. Find reuses disk discovery and shared source preparation to register partition roots with recognized `StartOSBackupsV2` metadata, without recursive folder discovery or modifying backup contents. Background inventory scans never register targets. The catalog atomically skips already registered filesystem-UUID/root pairs, retaining names and settings; a removed target can be rediscovered with a fresh identity. Boot consumes the historical private CIFS catalog into targets and password-only private credentials, preserving existing CIFS IDs. New IDs are GUIDs independent of connection details. Managed local shared volumes and targets are Btrfs subvolumes beneath the persistent `PACKAGE_DATA` partition at `shared-volumes/<id>` and `backup-targets/<id>`. `DATA_DIR` itself is a volatile mount scaffold. Their path owners also supply generated `managedStorage` directories for clients; the UI presents names rather than opaque catalog IDs or internal managed paths. Migration consumes the legacy private CIFS catalog; rollback to an older same-version development build without registered-target support is unsupported.

`backup::target::catalog` reuses shared source preparation and inspection DTOs. `PreparedFilesystem` shares managed-subvolume validation and filesystem delegation; concrete filesystem types preserve idmapped/shared-CIFS service policy and raw backup policy. `PinnedDirectory` owns confined descriptor opening and its stable path. Prepared callers retain their backing mount; observer reads retain the directory descriptor without mount-cache ownership. Managed local removal reserves the existing mount-cache slot, rejects active users and excludes new mounts through committed subvolume deletion, including nested subvolume roots. Shared-volume removal first revokes grants and detaches consumers. Registration survives failed or interrupted deletion for retry; successful deletion unregisters the source. The Btrfs cleaner reclaims the removed subvolume's space asynchronously. Boot converts ordinary local roots with the package-volume reflink/swap recovery owner before observers or services start. Obsolete pending local deletions recover as named, ungranted registrations during database normalization. External removal preserves contents. Observations reject stale source/credential snapshots and never recreate removed records. Backup, conversion and legacy-cleanup completion refresh target recovery metadata. Legacy CIFS add/update validate the share and existing unencrypted backup metadata before publishing a change; the target lifecycle RPCs retain unavailable configuration intent. Both reuse the target-content inspection owner, where capacity failures are diagnostic rather than a mount or metadata-validation failure. Mounted targets use the same metadata reader for initialization and recovery without opening another mount. The catalog requires validated contents before an unknown or unavailable target becomes available; folder-only status updates cannot clear metadata errors. The backup terminal-report owner persists completed background runs in private history with the target name captured at acceptance; `backup.history.list` pages them newest first. Synchronous request/setup rejection does not create a completed background run.

Backup-FS mount caching and trash sweeps use actual directory identity, including through pinned descriptors and aliases; a replacement at the same pathname does not reuse an old encrypted mount. Public source errors redact private cache identities.

`FileSystem::mount_hash` owns temporary-mount compatibility. CIFS keys include the caching policy used by the mount command: strict-cache readers and ordinary writers share a slot, while backup writers use a separate uncached mount. Compatible access upgrades preserve block-device sharing. Shared-service CIFS and backup CIFS retain separate runtime-policy identities.

## Backup conversion

`backup.target.convert` authenticates the selected target/server backup's existing credential and starts a background conversion. `PreparedConversion` owns the mount, data path and zeroizing credential; the worker retains only the database and shutdown receiver, not the full RPC context. The filesystem helper takes its existing exclusive store lock. Server shutdown cancels the helper before releasing the mount. Backup acceptance shares the per-target/server running-conversion check before publishing progress; the helper lock still arbitrates races, reversed starts and externally launched operations. Conversion reuses the authenticated pending marker and durable migration pointers on the backup, rather than trusting progress counters.

`ServerStatus.backupConversions` holds observed jobs keyed by the target's canonical string and server ID. `FullProgressTracker` streams phase/count updates through Patch-DB; the existing initialization reset clears these transient observations on restart. The helper's `conversion.pending` discovery hint lets another session or server offer explicit resumption. That hint and `StartOsRecoveryInfo.formatStatus` are unauthenticated observations, not permission to mutate a store.

`backup-fs-protocol` owns the portable helper messages shared by core and the Linux filesystem implementation. Core uses `Invoke::output_to` to consume bounded JSON progress lines and preserves stdin credential transport. The helper throttles progress output and aggregates corruption diagnostics; neither channel contains credentials.

## Storage availability

On Linux, `shared_volumes::watch` runs under the RPC context's existing cron ownership. Like the network interface watcher, it uses tiered recovery/observation loops and `Until` scopes. `BackgroundJobRunner::run_while` polls subscriptions, source checks, discovery and mounted probes on that same task; the observer spawns no task per source or query. Existing blocking-I/O helpers remain responsible for blocking syscalls. Session errors retry after one second without discarding pending checks or per-source backoff. An atomic PatchDB root watch filters registered-source snapshots, including private credentials, and ignores unrelated state. The observation tier waits for relevant events or its next health, retry or retirement deadline rather than polling every second. Mounted-health invalidations preserve a one-second minimum between scans, including while Unknown CIFS sources have pending probes. It subscribes to post-processing udev block events, kernel link/address/route events and Avahi SMB service announcements. Startup and source changes reuse `volumes.inspect` and `backup.target.inspect`; absent filesystem UUIDs become unavailable without a mount. Unavailable CIFS registrations receive bounded port-445 checks with backoff before a full inspection. Shared volumes require service grants for retries; registered backup targets need no grants. PatchDB availability changes use the existing shared-volume service callbacks. `Public.disks` holds the authoritative attached-drive inventory for the target and shared-volume drive dialogs. `disk::discovery` republishes it through one cancellable scan; context watch requests also refresh backup metadata after formatting, backup, conversion and legacy deletion. Shared volumes flatten `SharedVolumeInspection`, sharing availability, free space, error and check-time fields with the inspection RPC. The Volumes page requests checks for observations older than ten minutes and reads their results from PatchDB. Listener, scan and per-source jobs abort with their owning cron. Both registered collections share the existing running-job and retry owners; source-definition checks include private passwords and reject stale publications. Service loop-device updates do not trigger physical-drive discovery. Network events leave available, unmounted CIFS sources passive and do not bypass unavailable-source retry backoff. Automatic cold CIFS checks require a granted volume or a registered target; manual inspection remains explicit. Device events and source-definition changes can replace obsolete checks. Capacity errors remain observation data after a successful source open, rather than failing service mounts or volume backups.

`MountGuard` captures block-node identity; cache reuse rejects a removed or replaced node even when its pathname is reused. Weak observers do not retain mounts. Attachment clones start private before entering the target's propagation tree; detaching one attachment preserves other targets and replacements. Slot-locked retirement clears shared ownership so an old guard cannot unmount its replacement. Source checks compare the current source and credentials before publishing observations. `disk::mount::filesystem::cifs::health` owns kernel TCP/session/tree parsing and raw mountinfo correlation; unsupported layouts or conflicting matches remain indeterminate. `FileSystem::check_mount` checks both fresh mounts and cached reuse before access upgrades or caller I/O. Known-down CIFS rejects that request without retiring the existing mount; reconnection can reuse the same resource. Source and backup wrappers delegate this check to CIFS, and the mounted observer reuses one kernel snapshot per tick. `watch::mounted` reads kernel state and validates selected folders every thirty seconds, measuring capacity on existing mounts every three minutes. Policy-aware cache lookup finds both strict and uncached backup mounts without mounting unseen sources. A folder becomes available after a confined directory open; parent-filesystem health or capacity cannot clear a folder error. Indeterminate CIFS health also requires the existing-mount capacity fallback. Capacity and selected-folder checks have independent pending slots: a blocked capacity query cannot prevent folder recovery or observation of another registration. A five-second deadline publishes an observation while retaining the unfinished query, bounded to one capacity query per weak mount and one directory query per weak mount/selected-folder pair. When aliases share a directory, pending target initialization or recovery selects a metadata-validating check. Available targets use ordinary folder checks rather than periodic metadata scans. Validated contents remain pending until the source observation is ready and are consumed at publication; later capacity/kernel updates cannot replay old metadata to clear a subsequent validation error. Completed directory slots are pruned when no registration uses them; unfinished slots survive until completion or mount retirement. These slots extend the existing weak observer map, retaining neither mounts nor RPC contexts. Block events retire changed device sources; busy slots and failed retirement retain a one-second retry deadline while pending. Every retirement, including replacement by a mount caller, queues device invalidation until the observer consumes it.

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
