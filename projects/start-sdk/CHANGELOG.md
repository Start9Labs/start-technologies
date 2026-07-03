# Changelog

## 2.0.0 — StartOS 0.4.0-beta.10 (2026-06-07)

### Added

- `addSsl.upstreamCertValidation` controls how the OS reverse proxy validates the container's TLS certificate when it **rewraps SSL** (`addSsl` set AND the protocol's `secure.ssl === true`, e.g. `https`/`wss` — the OS terminates the client's TLS and opens a fresh TLS connection to the container). Omitted (the default) validates against the StartOS root CA, unchanged from before. `'disable'` skips validation entirely — for containers serving a self-signed cert on the trusted internal bridge. `{ certificate: '<pem>' }` validates against a supplied PEM certificate/chain instead of the root CA. Set it through `bindPort`'s `addSsl`, e.g. `multi.bindPort(443, { protocol: 'https', addSsl: { upstreamCertValidation: 'disable' } })`. Exported as `UpstreamCertValidation`
- **Init progress reporting via `FullProgressTracker` (auto-syncing).** Service code never calls a progress effect — and usually never calls `sync()` either. The init harness builds one root `FullProgressTracker` with the effects context baked in and passes it to every init handler as a third argument: `setupOnInit(async (effects, kind, progress) => …)`. Each handler adds its own phases (with its own names) to the shared tracker, unaware of the others. Add phases and update them (`phase.setTotal/setDone/complete`); **every update auto-reports in the background** to the "Installing" / "Updating" phase of the install. Auto-sync is coalesced — at most one report in flight and one queued, so a burst of updates collapses to the latest snapshot and promises never stack up. `tracker.sync()` is now an explicit *flush* (no args) that resolves once in-flight + queued reports have drained; the harness calls it before returning. Migrations receive the same tracker via `migrations.up`/`down`/`other` opts (`async ({ effects, progress }) => …`). A handler that didn't keep what `addPhase`/`addNestedPhase` returned can fetch it back by name with `tracker.getPhase(name)` (a `PhaseHandle` or, for a nested phase, a `FullProgressTracker`). `FullProgressTracker` is exported from `utils` (`utils.FullProgressTracker`). The underlying `effects.setInitProgress` / `effects.setBackupProgress` remain but are internal
- **Backup/restore progress mirrors the same structure.** `Backups.createBackup` and `restoreBackup` build an auto-syncing `FullProgressTracker` instead of calling the effect directly. The pre/post backup and restore hooks (`setPreBackup`, `setPostBackup`, `setPreRestore`, `setPostRestore`) now receive a `FullProgressTracker` as a second argument so custom work (DB dumps, etc.) can report sub-progress; restore progress flows through the init tracker since restore runs during init. Existing hooks that ignore the new argument keep working
- `PgDumpConfig.readyTimeout` / `MysqlDumpConfig.readyTimeout` make the post-start readiness wait in `Backups.withPgDump` / `Backups.withMysqlDump` configurable (milliseconds, matching the `gracePeriod` / `sigtermTimeout` convention; defaults 60000 Postgres / 30000 MySQL/MariaDB — unchanged from before). Each dump helper boots a throwaway DB server against the volume and polls until it accepts connections; a large or crash-recovering data directory can exceed the old hard-coded ceiling and fail the backup with "failed to become ready within N seconds" even though the live daemon — which gets a far longer `gracePeriod` — starts fine. Raise it to match, e.g. `withMysqlDump({ …, readyTimeout: 180000 })`
- `userspaceFilesystems` and `virtualNetworking` manifest flags split the former `nestedRuntime` flag into its two independent device grants. `userspaceFilesystems` mounts `/dev/fuse` for fuse-overlayfs storage (the rootless driver behind a nested OCI runtime). `virtualNetworking` mounts `/dev/net/tun` so a service can bring up kernel tun interfaces for VPN / WireGuard / tun-class workloads. The service LXC already retains `CAP_NET_ADMIN` within its user namespace via the standard `userns.conf` include, so no extra capability machinery is required — only the device node was missing
- **`SharedOptions.idmap` on volume / asset / dependency / backup mounts is now functional** (the field was previously declared but inert). Each entry is `{ fromId, toId, range? }` — map `range` consecutive ids (default `1`) from filesystem id `fromId` (u) to mountpoint id `toId` (k) — so a mount can present files under the uid/gid the service expects regardless of how they're stored on the host volume. The container's own LXC id-mapping is applied automatically and must **not** be included here. End-to-end support requires the StartOS 0.4.0-beta.10 host (7.0.7-backports kernel): the inner bind now runs through a new in-LXC `start-container mount` (`open_tree(OPEN_TREE_CLONE)` + `mount_setattr(MOUNT_ATTR_IDMAP)` + `move_mount`) rather than `mount --bind -oX-mount.idmap=…`, and `SubContainer.bind()` uses the same path. The standalone `IdMap` binding is removed (folded into `MountTarget.idmap`) and the host-side `IdMap::stack` workaround (which produced overlapping uid_map ranges on 6.x) is gone. The public `effects.mount(...)` signature is unchanged. Fixes [#3248](https://github.com/Start9Labs/start-technologies/pull/3248)
- `MultiHost.bindPortRange({ internalStartPort, externalStartPort, numberOfPorts })` reserves a contiguous TCP+UDP port range in a single call, intended for real-time / WebRTC servers (coturn, RTP, SIP) that need a public range. The whole range is allocated atomically; any partial collision with already-bound external ports is a hard error (no shifted-range fallback). A range is **2–500 ports**; for a single port use `bindPort`. `externalStartPort` may differ from `internalStartPort`: the forward maps the external range onto the internal range by offset (port-preserving when the two bases are equal). Returns a `RangeOrigin`, on which `.export(sdk.createRangeInterface(effects, { id, name, description, scheme? }))` registers the range's single, restricted `api` service interface — no SSL and no `masked`/`username`/`path`/`query`/`schemeOverride`. A range exposes **exactly one** interface (distinct endpoints are separate `bindPortRange` calls); the optional `scheme` is a transport prefix (e.g. `tcp` for bitcoin ZMQ endpoints) that most ranges (coturn RTP, FTP data) omit. The OS persists the range as a single `RangeBindInfo` record under `Host.binding_ranges` (not N entries in `Host.bindings`), carrying the exported interface and installing one nft rule per chain covering the whole range (`PortForward` gains a `count` field, defaulting to 1 for back-compat). Backed by the effects `effects.bindRange(...)` and `effects.exportRangeServiceInterface(...)` (requires StartOS with the matching backend handlers — landed in this PR). Fixes [#3269](https://github.com/Start9Labs/start-technologies/issues/3269)
- `sdk.Daemons.dynamic(fn)` builds a reactive `main` entrypoint whose daemon set is a function of on-disk state. The supplied builder returns a regular `sdk.Daemons.of({ effects }).addDaemon(...)` chain (now record-then-materialize — see below), and the SDK diffs its entries against the running set on every `effects.constRetry` trigger (typically fired by a `FileHelper.read().const(effects)` watcher). Per id: absent → present **start**, present → absent **stop**, same `configHash` **leave alone**, different `configHash` **restart**. Dependents of any restarted or stopped daemon are also restarted to keep `requires` wiring consistent. Re-runs coalesce while one is in flight. Designed for multi-tenant packages like the registry-portal s9pk that add, rename, and delete sub-instance daemons without restarting the service
- `configHash` covers the subcontainer descriptor (`imageId`, `sharedRun`, `name`, structural `mounts.build()`), exec (`command`, `env`, `cwd`, `user`, `runAsInit`, `sigtermTimeout`), `requires` (sorted), and the structural parts of `ready` (`display`, `gracePeriod`). Closures (`ready.fn`, `ready.trigger`, function-form `exec.fn`) and pre-built `Daemon` instances are intentionally excluded — surface a value through one of the hashed fields if you want the reconciler to react to it changing
- `sdk.SubContainer.eager(...)` creates a SubContainer with its filesystem materialized immediately. Use when you need `createFs` failures to surface at the construction site instead of at first method call, or when you need sync access to `rootfs` / `guid` / `subpath()` before running any methods
- `SubContainerLazy.eager(): Promise<SubContainerEager<M>>` forces materialization on a lazy handle and returns the underlying eager subcontainer for callers that need the narrowed sync interface
- New compile-time validator `ValidateExVerRange<T>` validates exver version-**range** string literals across the full range grammar (comparison / caret / tilde anchors over `[#flavor:]upstream[:downstream]` specs, `&&` / `||`, parentheses, negation, `#flavor` atoms, and `*`), catching a malformed version atom anywhere in the expression. It now guards the two places a range literal is written: a dependency's `versionRange` (via `sdk.setupDependencies`) and the keys of `migrations.other`, so typos like `'>=2.f'` or `'^28 || 30.x:0'` fail at `tsc` time instead of at runtime. Purely additive types — runtime parsing is unchanged, and structural mistakes (unbalanced parens, a dangling operator) are still left to runtime validation
- Actions gain an optional `access` field on the metadata passed to `Action.withInput` / `Action.withoutInput` — `'public' | 'dependent' | 'user'`, defaulting to `'user'` when omitted. It controls who may invoke the action **directly** via `effects.action.run({ packageId, actionId, input })`: `'public'` — any installed package; `'dependent'` — only services that declare this package as a current dependency; `'user'` — only the user (other services must `effects.action.createTask(...)`). Direct cross-package runs against an action whose access denies the caller are rejected. Direct runs still go through the owning service and honor the action's `visibility` (disabled) and `allowedStatuses` checks
- `detach()` on `SubContainer`, `Daemon`, and `Daemons` (plus the object returned by `Daemons.dynamic(...).build()`) severs the instance from the effects context it was created under, so that context leaving (`onLeaveContext`) no longer terminates the daemon(s) or destroys the subcontainer(s). This is the escape hatch for the now-deterministic context cleanup (see _Fixed_): a short-lived context — typically an **action** — can build a `Daemons` chain (or a lone `Daemon`/`SubContainer`), `detach()` it, and let it outlive the RPC that started it. A `Daemon` takes ownership of its subcontainer's teardown at construction (detaching it from the per-context cleanup net so only the daemon's `term()` — stop process, then destroy — can tear it down), so detaching the daemon/chain only governs its own self-term. After detaching, the caller owns the lifecycle and must call `term()` (or `destroy()`) explicitly. Idempotent
- The published package now ships `s9pk.mk` and `tsconfig.base.json` at its root, so a package can consume the canonical build plumbing by reference instead of vendoring copies: its `Makefile` does `include node_modules/@start9labs/start-sdk/s9pk.mk` and its `tsconfig.json` does `"extends": "@start9labs/start-sdk/tsconfig.base.json"`. Bumping the SDK then delivers build-system fixes automatically, ending the per-package drift of these files. The `s9pk.mk` `install` / `publish` targets resolve the deploy host and registry through `start-cli` (the packaging-workspace `.startos/config.yaml` `host`/`registry` profiles, or `-H` / `-r`) instead of parsing `~/.startos/config.yaml` directly, so the plumbing works with the `s9pk init-workspace` model out of the box

### Changed

- **Breaking — `sdk.SubContainer.of(...)` is now lazy by default.** Returns a `SubContainerLazy<M>` *synchronously* (no `Promise<>` wrapper) whose filesystem is materialized on first method call. `rootfs` / `guid` / `subpath()` on the unified `SubContainer<M>` interface widen to `T | Promise<T>`; the concrete classes narrow (`SubContainerEager` to `T`, `SubContainerLazy` to `Promise<T>`). Code holding the generic interface must `await` those accessors; code holding `SubContainerEager` keeps sync access. Migration: most callers already use only async methods (`.exec`, `.writeFile`, `.spawn`, `.launch`) and need no change beyond dropping the redundant `await` on `SubContainer.of(...)`; callers that read `.rootfs` / `.guid` / `.subpath()` add an `await` or switch to `sdk.SubContainer.eager(...)`. The lazy default enables `Daemons.dynamic`'s "leave alone is load-bearing" guarantee: unchanged daemons across reconciles never materialize a fresh subcontainer
- **Breaking — `SubContainerOwned` / `SubContainerRc` collapsed into `SubContainerEager`.** The reference-counted handle (`SubContainerRc`, `.rc()`, `.isOwned()`) is replaced by an internal hold-count on the unified `SubContainer`. Multiple consumers each call `sub.hold()` (returns a release fn); the container's `destroyFs` fires when `destroy()` has been called and the last hold is released, in either order. `Daemon.start()` takes its own hold for the daemon's lifetime and releases it on `term()`. The `destroySubcontainer` flag on `Daemon.term` / `HealthDaemon.term` is gone — `Daemons.term()` calls `destroy()` on each unique subcontainer in its entries, and the hold machinery handles sharing safely
- **Breaking — `Daemon.sharesSubcontainerWith` removed.** Two `Daemon`s share a subcontainer when constructed with the same `SubContainer` instance (compare `daemon.subcontainer.identity`). Daemons' internal "should I destroy this subc?" branching is gone — it always calls `destroy()`, and hold-count decides
- **Breaking — `Daemons` is record-then-materialize.** `.addDaemon()` / `.addOneshot()` / `.addHealthCheck()` append a recorded entry without constructing `HealthDaemon` or `Daemon`. `Daemons.build()` walks the entries and constructs the chain in one pass. Functionally identical for `setupMain` callers — the original "construct on addDaemon, kick off on build" timing is invisible because nothing actually starts until `build()`'s `updateStatus()` calls anyway. The change is load-bearing for `Daemons.dynamic`, which needs to diff entries without paying the cost of building them eagerly each reconcile
- **Breaking — `SubContainer` adds `identity: symbol`.** Sync, stable handle preserved across `SubContainerLazy.eager()` materialization. Use for sharing checks that must work before materialization (replaces `Daemon.sharesSubcontainerWith`'s previous `guid` comparison, which couldn't fire pre-materialization)
- Container-runtime updated: `SubContainerOwned` → `SubContainer.eager`; `SubContainerRc<M>` → `SubContainer<M>`; `daemon.subcontainerRc()` → `daemon.subcontainer`
- **zod bumped `4.3.6` → `4.4.3`** (it was emergency-pinned to exactly `4.3.6` in 1.4.1). zod 4.4.0 tightened object parsing so a required key wrapped in `.catch()` threw `"expected nonoptional, received undefined"` on a *missing* key — which broke the SDK at import (`actions/input/inputSpecConstants.ts` parses its SMTP shapes at module load) and the `.merge({})` seed pattern that every FileHelper-based package relies on. zod **4.4.3** (#5939, #5941) restores the 4.3.6 behavior, so the pin is exactly `4.4.3` — **not** `^4.4.0`, which would admit the still-broken 4.4.0–4.4.2. The `zExport` loose-object patch and FileHelper are unaffected (verified against the shipped build). Downstream: packages taking SDK 2.0 inherit zod 4.4.3 transitively and need **no** schema change for the catch-on-missing case; the other cumulative 4.4.0 fixes (stricter `z.base64()` / `z.httpUrl()`, `.merge()` throwing on schemas with refinements, materialized tuple defaults) may surface as `tsc` errors only in packages that use those specific APIs
- **TypeScript bumped `^5.9.3` → `^6.0.3`, and the build migrated off the deprecated `moduleResolution: "node"` (node10).** TS 6.0 deprecates node10 resolution (removed entirely in TS 7.0). The SDK's own `base` / `package` tsconfigs now use `moduleResolution: "bundler"` (keeping `module: "commonjs"` — emitted CJS is byte-identical and the published `.d.ts` API surface is unchanged) plus an explicit `rootDir` (TS 6.0 changed the default to the tsconfig directory). The shipped `tsconfig.base.json` that packages `extends` now uses `target: "ES2022"`, `moduleResolution: "bundler"` + `module: "preserve"`, `types: ["node"]`, and `rootDir: "${configDir}"`, so a package needs only `extends` + `include` and nothing else: node globals are not auto-included under bundler resolution; the prior `ES2018` target predated `Object.fromEntries` and other ES2019+ lib APIs packages rely on; and TS 6.0 requires an explicit `rootDir` for the `ncc` emit (the `${configDir}` template resolves to each extending package's own directory). Putting the fleet on modern resolution before TS 7, no SDK source changes were required
- **Breaking — `input-not-matches` task input split into `accept` (a list) and `set`.** `TaskInput` was `{ kind: 'partial', value }`, where the single `value` both decided whether the task was satisfied (the current action input had to be a superset of it) and prefilled the action form when the user ran the task. It is now `{ kind: 'partial', accept: DeepPartial<Input>[], set: DeepPartial<Input> }`: the task is satisfied when the current input matches **any** entry in `accept`, and when none match the task is shown and prefills `set`. This lets a package accept several already-good configurations while still pushing a single recommended value when none of them hold — e.g. accept either of two valid network modes, but set the preferred one otherwise. The cross-package critical-conflict guard now activates only when the input conflicts with **every** `accept` entry. Migration: replace `value: X` with `accept: [X], set: X` for identical behavior to 1.x. Already-published s9pks built against the pre-2.0 SDK keep working without a rebuild — the StartOS host still accepts the legacy `{ kind: 'partial', value }` payload over the effects socket and normalizes it to `accept: [value], set: value`
- **Breaking (internal) — zod loose-object handling unified onto a single `z` export; the global `require.cache` patch was removed.** The SDK makes `z.object()` preserve unknown keys (so `FileHelper` models that declare only a subset of a config file round-trip the rest). This was previously done by *two* mechanisms: a shadow `z` (the one exported from `@start9labs/start-sdk`) plus a walk over Node's `require.cache` that mutated the raw `zod` module so the SDK's own internals — which `import { z } from 'zod'` — also got loose objects. The cache walk was fragile (it duck-typed the module cache and silently no-op'd outside CommonJS). It is removed: the SDK's internal modules now import `z` from the shadow directly, and `FileHelper`'s structured factories (`json` / `yaml` / `toml` / `ini` / `env` / `xml`) now deep-loosen their shape explicitly via `z.deepLoose(shape)`, making unknown-key preservation a property of the file-model boundary rather than a global default. **Packaging code is unaffected:** `import { z } from '@start9labs/start-sdk'` still yields loose-by-default objects and every package's file models keep preserving undeclared on-disk keys. The only behavioral change is for code that imported `z` directly from `zod` and relied on the SDK to have patched it — it must import `z` from `@start9labs/start-sdk` instead. No StartOS packaging code does this (only vendored upstream application sources import `zod` directly, and they use their own copy)

- **Breaking — service interfaces moved onto their binding; `sdk.serviceInterface.*` accessors replaced by `sdk.host.*`.** A service interface is no longer a flat entry on `PackageDataEntry.serviceInterfaces` (that field is removed) — it now lives on the binding that exported it: `Host.bindings[internalPort].interfaces` (a `ServiceInterfaceId`-keyed map; a port may back several) for single-port `Origin.export`, and `Host.bindingRanges[internalStartPort].interface` for a range's `RangeOrigin.export`. Correspondingly the SDK drops `sdk.serviceInterface.{getOwn, get, getAllOwn, getAll}` and adds `sdk.host.getOwn(effects, hostId)` / `sdk.host.get(effects, { hostId, packageId? })`, which return the reactive `Host` (same `const`/`once`/`watch`/`onChange`/`waitFor` read strategies). Both accessors take an optional `map` (and `eq`, default deep-equal) selector — matching the old `sdk.serviceInterface.*` — so `const()` can re-run on a change to a single child attr of the host rather than wholesale on the entire host. Reach an interface by walking the host — `Object.values(host.bindings).flatMap(b => Object.values(b.interfaces))` (or `host.bindingRanges[start].interface`). Each single-port interface's `addressInfo` comes back **already filled** — carrying the `filter`/`format`/`nonLocal`/`public`/`bridge`/`toUrl` helpers directly (e.g. `iface.addressInfo.format('url')`), so no separate `utils.filledAddress(host, …)` call is needed. The win: a binding's host is now reachable even when it exports no interface.

### Fixed

- Every materialized `SubContainer` is now torn down when the effects context that created it leaves (`onLeaveContext`), instead of lingering until GC eventually runs its `Drop` finalizer. This closes the gap where a subcontainer created in `main` (or any context) but never attached to a daemon — e.g. an ad-hoc setup/bootstrap container — could outlive its context. One cleanup hook is armed per effects object and each subcontainer removes itself on `destroy()`, so repeated short-lived containers (`withTemp`, per-poll health checks) don't accumulate registrations; teardown still routes through the hold-aware `destroy()`, so a container held by a running daemon defers until the daemon's own shutdown releases it
- `filledAddress` now excludes mDNS (`.local`) addresses whose gateways have no enabled LAN IP. An mDNS name resolves only via a LAN IP on a shared gateway, so when every such IP is disabled the `.local` address is unreachable — it was previously still reported as available, which let the StartOS UI offer (and launch) an unresolvable `.local` URL even though the address table showed it disabled. The rule is now exported as `utils.mdnsResolvable(hostname, enabledHostnames)`, shared between the SDK's reachable-address filter and the UI's address table so the two stay consistent

### Removed

- **Breaking — `nestedRuntime` manifest flag removed**, with no compatibility alias. It conflated two unrelated device grants (`/dev/fuse` for fuse-overlayfs storage and `/dev/net/tun` for kernel tun interfaces). Replace with `userspaceFilesystems` (nested OCI runtimes) and/or `virtualNetworking` (kernel tun interfaces). Packages must republish: a host updating StartOS parses an old `nestedRuntime` field as absent, so both new flags default to `false`
- **Breaking — package `alerts` manifest field removed**, with no compatibility alias. Packages can no longer define `install` / `update` / `uninstall` / `restore` / `start` / `stop` alert messages (the confirmation prompts StartOS showed before those lifecycle actions). Drop the `alerts` block from `setupManifest`; a host updating StartOS parses any leftover `alerts` field as absent and StartOS no longer surfaces these prompts. Built-in confirmations for destructive actions (uninstalling, stopping a service with active dependents) are unaffected
- `SubContainerOwned`, `SubContainerRc`, `SubContainer.rc()`, `SubContainer.isOwned()` — folded into the unified `SubContainerEager` / `SubContainerLazy` with hold/release lifecycle
- `Daemon.subcontainerRc()`, `Daemon.markManaged()`, `Daemon.sharesSubcontainerWith()` — superseded by `daemon.subcontainer` (public readonly) and the hold-count model
- `destroySubcontainer` option on `Daemon.term` / `HealthDaemon.term` — `Daemons` calls `subcontainer.destroy()` for each unique subc on shutdown, and the hold-count decides actual timing
- **Breaking — `sdk.serviceInterface.{getOwn, get, getAllOwn, getAll}` and `PackageDataEntry.serviceInterfaces`** — interfaces moved onto their binding; use `sdk.host.{getOwn, get}` and walk the host's bindings (see _Changed_). The underlying `utils.getServiceInterface` / `utils.getServiceInterfaces` helpers and the `GetServiceInterface` reader class are removed with them — `sdk.host.get`/`getOwn` returns a host whose interface `addressInfo`s are already filled (`filter`/`format`/`nonLocal`/`public`/`bridge`/`toUrl`). The `get_service_interface` **RPC/effect is retained** for backwards compatibility with packages built against older SDKs

## 1.5.3 — StartOS 0.4.0-beta.9 (2026-05-20)

### Fixed

- `Backups.withMysqlDump` no longer wedges the entire backup (and the StartOS host) on the database-shutdown step. After the dump it stopped mysqld with `kill $PID && tail --pid=$PID -f /dev/null`, but the daemonized (mysql) / backgrounded (mariadb) mysqld is never reaped in the dump subcontainer, so it lingers as a zombie that keeps its PID and `tail --pid` waited on it forever. The subcontainer was then never torn down and the backup hung until the box was rebooted — often surfacing afterward as a misleading `ENOTCONN … mkdir` notification as mounts were unwound under the stuck operation. Shutdown now waits for mysqld to reach the zombie (`Z`) state or vanish (treating either as "exited"), with a bounded SIGKILL fallback so it can never deadlock again. Applies to both backup and restore. `withPgDump` was unaffected (it uses `pg_ctl stop -w`, which reaps cleanly). The `withMysqlDump` packages (ghost, mempool) should rebuild against this SDK

## 1.5.2 — StartOS 0.4.0-beta.9 (2026-05-15)

### Fixed

- `Backups.withPgDump` and `Backups.withMysqlDump` now stage the dump file in the subcontainer rootfs (`/tmp/<db>-db.dump`) and `cp` it through to the backup target, on both backup and restore. Previously `pg_dump` / `mysqldump` wrote directly to the `backup-fs` FUSE bind mount, where their writes were silently dropped — the process exited 0 but the resulting `<db>-db.dump` was 0 bytes. Every package using these helpers (immich, spliit, synapse, nextcloud, btcpayserver, mempool, ghost) has been shipping empty database dumps since the helpers were introduced; bump to this SDK and rebuild the s9pk to recover. Plain `cp` writes through the FUSE work, so staging through `/tmp` produces an identical on-disk dump file at the same path — no restore-format change

## 1.5.1 — StartOS 0.4.0-beta.9 (2026-05-13)

### Fixed

- `GetActionInputType` (used to infer the input shape for `TaskOptions` / `task()` partials) now matches against `ActionInfo` instead of `Action`. The conditional previously tested `A extends Action<...>` while `A` was constrained to `ActionInfo<...>`, so inference fell through to `never` and `TaskInput.value` collapsed to `DeepPartial<never>`. Tasks built from `ActionInfo` now infer their input type correctly
- `BindOptions.addSsl` is now `Partial<AddSslOptions>` for protocols without SSL variants, matching the type already used on the SSL-variant branch. The two branches of the discriminated union are now consistent, so callers can omit individual SSL option fields regardless of which protocol they're binding

## 1.5.0 — StartOS 0.4.0-beta.9 (2026-05-08)

### Added

- `sdk.notification.create(effects, options)` lets a package post a notification into the StartOS notifications panel, alongside the ones StartOS itself generates (e.g. on backup completion). `options` is `{ level, title, message, data? }`: omit `data` for a plain notification (panel row only) or pass markdown text for `data` to attach a long-form body the UI renders in a "View Details" modal (release notes, post-update changelogs, structured error reports). The host forces `packageId` to the calling service's id, so a package can't spoof another package
- Backed by a new effect `effects.notification.create(...)` (requires StartOS 0.4.0-beta.9 with the matching backend handler)

### Changed

- Minimum StartOS version bumped to `0.4.0-beta.9` — required for the `notification.create` effect added in this release

## 1.4.3 — StartOS 0.4.0-beta.8 (2026-05-07)

### Fixed

- `FileHelper.produce` no longer leaks an `abort` listener on its parent `AbortSignal` per `fs.watch` event. The listener was registered inside the `while` loop and never detached, so after ~11 file changes during a single `.const()` lifetime Node emitted `MaxListenersExceededWarning: Possible EventTarget memory leak detected. 11 abort listeners added to [AbortSignal]`. The listener is now scoped to a single iteration via `try`/`finally` so it is removed before the next iteration registers its own. Fixes [#3182](https://github.com/Start9Labs/start-technologies/issues/3182)

## 1.4.2 — StartOS 0.4.0-beta.8 (2026-05-06)

### Changed

- `VersionGraph.uninit` now throws when handed a range target with no satisfying version, instead of silently returning without advancing the data version. The previous silent behavior masked host bugs that produced contradictory targets (e.g. an `AND` of a flavored on-disk range with a flavor-less destination's `canMigrateFrom`) — uninit would skip the registered cross-flavor migration, leave the on-disk range untouched, and the destination's init would dead-end with `cannot migrate from <range> to <concrete>` with no signal at the source. Throwing surfaces the contract violation at the source where it can be diagnosed. Packages whose host correctly computes uninit targets are unaffected

## 1.4.1 — StartOS 0.4.0-beta.8 (2026-05-04)

### Fixed

- Pin `zod` to exact version `4.3.6`. Zod 4.4 changed `.catch()` to no longer fire on missing object keys (only on present-but-invalid values), which silently breaks every package's `.merge({})` seed pattern — required fields with `.catch()` defaults throw `"expected nonoptional, received undefined"` instead of falling back to the default. The previous `^4.3.6` range allowed packages installed after 4.4.0 dropped to pick up the new behavior on `npm install`. Pinning ensures consistent behavior across all packages regardless of when they were installed

## 1.4.0 — StartOS 0.4.0-beta.8 (2026-05-03)

### Added

- `FileHelper.yaml` accepts an optional `options` argument that is threaded into both `YAML.stringify` and `YAML.parse`. Enables custom YAML tags (`!include`, `!secret`, `!include_dir_*`, etc.) to round-trip through the file model. Pass `customTags` to register tag handlers; pair the handler's `resolve` (returns a JS value during parse) with `identify`/`stringify` (re-emits the tag during write) to keep tag information across read-modify-write cycles. Other yaml options (e.g. `indent`, `lineWidth`, `aliasDuplicateObjects`) are accepted on the same object

### Changed

- **Breaking:** `FileHelper.yaml`'s transformers overload now takes `options` at position 3 and `transformers` at position 4, matching the argument order of `FileHelper.ini` and `FileHelper.xml`. Call sites that passed `transformers` as the third positional argument must be updated. The simple form (`FileHelper.yaml(path, shape)`) is unchanged

## 1.3.4 — StartOS 0.4.0-beta.8 (2026-05-02)

### Added

- `ProxyAuth` and `BasicCredential` bindings re-exported from `interfaces/Host`. Pass `auth` on `AddSslOptions` to put a `Bearer` or `Basic` gate in front of an HTTPS bind; the OS reverse proxy validates `Authorization` before forwarding upstream and (for `Basic`) injects the matched username as `X-Forwarded-User`. Setting `auth` implies HTTP-aware proxying. Optional `realm` controls the `WWW-Authenticate` challenge string

### Changed

- Minimum StartOS version bumped to `0.4.0-beta.8` — required for the reverse-proxy auth gate added in this release

## 1.3.3 — StartOS 0.4.0-beta.6 (2026-04-24)

### Fixed

- `checkPortListening` reads `/proc/net/{tcp,tcp6,udp,udp6}` via `fs.promises.readFile` instead of `cat` through `child_process.exec`. The prior implementation failed with `stdout maxBuffer length exceeded` on hosts where `/proc/net/tcp` exceeded 1 MB, causing false negatives on busy boxes

## 1.3.1 — StartOS 0.4.0-beta.6 (2026-04-21)

### Changed

- Minimum StartOS version bumped to `0.4.0-beta.6`

## 1.3.0 — StartOS 0.4.0-beta.4 (2026-04-21)

### Added

- `nullToUndefined(obj)` utility and `NullToUndefined<T>` type — recursively convert `null` values to `undefined` through objects and arrays

### Fixed

- Backup `rsync` no longer passes `--no-inc-recursive`; the pre-scan it forced was timing out on large backups. Accurate progress percentages go away as a side effect — to surface progress in the future, count files up front and compute percentage from bytes/files seen
- `withPgDump` pre-backup `touch` and `chown` of the dump file now fail loud instead of silently swallowing errors. Previously, if the backup target couldn't be written or chowned (e.g. a filesystem that doesn't honor Unix ownership), `pg_dump` would hit a confusing `Permission denied` on open instead of surfacing the underlying cause

## 1.2.0 — StartOS 0.4.0-beta.4 (2026-04-17)

### Added

- `footnote: string | null` on all scalar value specs (`text`, `textarea`, `number`, `color`, `datetime`, `toggle`, `select`, `multiselect`) — supplementary text rendered persistently beneath the field. Useful for showing upstream defaults, units, or any dev-authored clarification
- `Value.triState(...)` / `Value.dynamicTriState(...)` — a three-state toggle (boolean toggle with a neutral middle position), rendered as three icon buttons (✕ / — / ✓). Left outputs `false`, right outputs `true`, middle outputs `null`

## 1.1.0 — StartOS 0.4.0-beta.4 (2026-04-15)

### Added

- `trigger.statusTrigger(defaultMs, overrides?)`: single trigger that maps each `HealthStatus` to a polling interval, replacing `changeOnFirstSuccess`, `successFailure`, and `lastStatus`
- TSDocs across the public daemon/health-check surface: `Ready`, `addDaemon`, `addOneshot`, `addHealthCheck`, `runUntilSuccess`, the `trigger` namespace, `healthCheck` helpers, and `HealthCheckResult`

### Changed

- Converted `VersionInfo`, `Action`, `Actions`, and `FileHelper` from classes to interfaces with companion objects, so services importing types across differing SDK versions no longer hit "type X does not match type X" nominal-typing errors

### Fixed

- Default daemon trigger now polls quickly for `starting`/`waiting`/`failure` and slowly for `success`/`loading`/`disabled`; previously `changeOnFirstSuccess` only transitioned on `success`, so a daemon stuck in `loading` (e.g. during Bitcoin IBD) was polled every 1s indefinitely

### Removed

- `trigger.changeOnFirstSuccess`, `trigger.successFailure`, `trigger.lastStatus` (use `statusTrigger` instead)

## 1.0.0 — StartOS 0.4.0-beta.0 (2026-03-31)

### Changed

- **Breaking:** Minimum StartOS version bumped to `0.4.0-beta.0` (from `0.4.0-alpha.23`) — marks the transition out of alpha

### Added

- `GenerateCertificateParams` / `GenerateCertificateResponse` bindings for certificates signed by the root CA
- `RestartReason` binding and `ServerStatus.restart` field for unified, reason-specific restart notifications
- `PackageVersionInfo.satisfies` field so dependency breakage checks can match flavored package versions against flavorless ranges

### Fixed

- `deepEqual` now handles non-plain objects by reference instead of walking them as structural values

## 0.4.0-beta.66 (2026-03-24)

- **Breaking:** `withPgDump()` replaces `pgdata` with required `mountpoint` + `pgdataPath`
- Passwordless/trust auth support for `withPgDump()` and `withMysqlDump()`
- New options: `pgOptions` for postgres, `mysqldOptions` for mysql/mariadb
- Fixed MariaDB backup/restore support

## 0.4.0-beta.65 (2026-03-23)

### Added

- `Backups.withPgDump()`: dump-based PostgreSQL backup using `pg_dump`/`pg_restore`, replacing raw volume rsync of PG data directories
- `Backups.withMysqlDump()`: dump-based MySQL/MariaDB backup using `mysqldump`/`mysql`
- Password configs accept `string | (() => string | Promise<string>)` for deferred resolution during restore

## 0.4.0-beta.63 — StartOS v0.4.0-alpha.22 (2026-03-22)

### Fixed

- Fixed `createTask` failing when input values are undefined
- Fixed daemon lifecycle cleanup and error logging improvements
- Replaced fire-and-forget restart loop in `Daemon` with tracked `AbortController`
- Fixed graceful shutdown for subcontainer daemons
- Fixed rsync progress regex never matching, spamming logs during backup
- Fixed rsync backup bugs and optimized flags for encrypted CIFS targets
- Fixed types in `inputSpecConstants`, `StartSdk`, and exports

## 0.4.0-beta.62 (2026-03-19)

### Fixed

- Fixed `Value.dynamicSelect` and `Value.dynamicMultiselect` crashing with `z.union([])` when `values` is empty (zod v4 compatibility)

### Added

- `FileHelper.xml`: file helper for XML files using `fast-xml-parser`
- `smtpShape`: typed zod schema for persisting SMTP selection in store file models, replacing direct use of `smtpInputSpec.validator` which caused cross-zod-instance errors

## 0.4.0-beta.61 — StartOS v0.4.0-alpha.21 (2026-03-16)

### Fixed

- Fixed bug where leaving the effect context triggered consts

## 0.4.0-beta.60 — StartOS v0.4.0-alpha.20 (2026-03-16)

### Added

- Tunnel TS type exports and port forward labels
- Secure Boot MOK key enrollment fields in `SetupInfo`

### Changed

- Consolidated `Watchable` base class with generic `map`/`eq` support; renamed `call` to `fetch`
- Moved `GetServiceManifest` and `GetSslCertificate` from `package/` to `base/`
- Simplified `getServiceInterface`, `getServiceInterfaces`, `GetOutboundGateway`, `GetSystemSmtp`, and `fileHelper` using `Watchable` base class
- Simplified SDK Makefile with rsync

### Fixed

- Added `restart_again` flag to `DesiredStatus::Restarting`

## 0.4.0-beta.59 — StartOS v0.4.0-alpha.20 (2026-03-06)

### Added

- Support for preferred external ports besides 443
- Bridge filter kind on service interfaces

### Fixed

- Merge version ranges when adding existing package signer
- Task fix for action task system

## 0.4.0-beta.56 — StartOS v0.4.0-alpha.19 (2026-02-02)

### Added

- `getOutboundGateway` effect and SDK wrapper
- Improved service version migration and data version handling
- `zod-deep-partial` integration with `partialValidator` on `InputSpec`
- SMTP rework with improved provider variants and system SMTP spec

### Changed

- Migrated from `ts-matches` to `zod` across all TypeScript packages
- Builder-style `InputSpec` API with prefill plumbing
- Split `row_actions` into `remove_action` and `overflow_actions` for URL plugins

### Fixed

- Scoped public domain to single binding and return single port check
- Preserved `z` namespace types for SDK consumers
- `--arch` flag falls back to emulation when native image unavailable

## 0.4.0-beta.54 — StartOS v0.4.0-alpha.18 (2026-01-27)

### Added

- Device info RPC
- Hardware acceleration and NVIDIA card support on nonfree images

### Changed

- Consolidated setup flow
- Improved SDK abort handling and `InputSpec` filtering

## 0.4.0-beta.49 — StartOS v0.4.0-alpha.17 (2026-01-10)

### Added

- JSDoc comments on all consumer-facing APIs
- StartTunnel random subnet support
- Port 80 to 5443 tunnel mapping

### Fixed

- `EffectCreator` type corrections
- Allow multiple equal signs in ENV `FileHelper` values
- Miscellaneous alpha.16 follow-up fixes

## 0.4.0-beta.45 — StartOS v0.4.0-alpha.16 (2025-12-18)

### Added

- `map` and `eq` on `getServiceInterface` watcher
- Flavor-aware version range handling

### Changed

- Refactored `StatusInfo` types
- Improved shutdown ordering for daemons
- Improved StartTunnel validation and garbage collection

## 0.4.0-beta.43 — StartOS v0.4.0-alpha.15 (2025-11-26)

### Fixed

- Minor bugfixes for alpha.14

## 0.4.0-beta.42 — StartOS v0.4.0-alpha.14 (2025-11-20)

### Fixed

- Bugfixes for alpha.13

## 0.4.0-beta.41 — StartOS v0.4.0-alpha.13 (2025-11-15)

### Fixed

- Bugfixes for alpha.12

## 0.4.0-beta.40 — StartOS v0.4.0-alpha.12 (2025-11-07)

### Added

- StartTunnel integration
- Configurable `textarea` rows in `InputSpec`

## 0.4.0-beta.39 — StartOS v0.4.0-alpha.11 (2025-09-24)

### Added

- Gateway limiting for StartTunnel
- Improved copy UX around Tor SSL

### Changed

- SDK type updates and internal improvements
