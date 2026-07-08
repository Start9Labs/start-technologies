# Architecture — StartOS OS product

This document covers the **OS product** (`start-os/`). For the monorepo as a
whole, see [../../ARCHITECTURE.md](../../ARCHITECTURE.md).
For the Rust backend internals, see the `start-core` crate at
[../../shared-libs/crates/start-core](../../shared-libs/crates/start-core).

## Place in the monorepo

`start-os/` is the package `start-os` (binaries `startbox` + `start-container`):
a thin wrapper that assembles the StartOS server operating system from shared
building blocks. It contributes the entry points and the OS-specific surface;
almost all backend logic is in the `start-core` crate, and it is consumed by the
root OS image build. Sibling product crates (`start-cli`, `start-registry`,
`start-tunnel`) are analogous wrappers over the same `start-core` lib.

## Binaries

`Cargo.toml` declares package `start-os` with two binaries, both depending on
`start-core` (imported as the `start_core` lib):

- **`startbox`** (`src/bin/startbox.rs`) — a multi-call applet enabling the
  `startd` daemon, the `start-cli` client, and the `unshare-userns` applet. It
  also embeds the built web UIs at compile time via `include_dir!`
  (`web/dist/static/ui`, `web/dist/static/setup-wizard`) and the Patch-DB UI
  seed (`web/patchdb-ui-seed.json`, or `*.beta.json` under the `beta` feature).
  Because the UIs are embedded, the web build must run before the Rust build —
  the `Makefile` encodes this ordering.
- **`start-container`** (`src/bin/start-container.rs`) — runs **inside** each
  service LXC. It enables `start-container` plus `unshare-userns`. The latter
  must be a multi-call applet (not a CLI subcommand): it calls
  `unshare(CLONE_NEWUSER)`, which the kernel rejects on a multi-threaded
  process, and the applet entry stays single-threaded.

Cargo features (`beta`, `console`, `dev`, `test`, `unstable`) forward to the
matching `start-core` features.

The sibling product crates (`start-cli`, `start-registry`, `start-tunnel`) are
analogous thin wrappers over the same `start-core` lib; the OS bins above are
this product's contribution.

## Web

Two Angular 22 apps live under `web/`:

- `ui/` — the admin dashboard (Angular project `ui`).
- `setup-wizard/` — the first-boot setup flow (project `setup-wizard`).

They are part of the single Angular workspace rooted at `../../` (the repository root)
(the root `angular.json` points each project's `root` at `projects/start-os/web/...`).
They consume the shared `@start9labs/shared` and `@start9labs/marketplace` libs
from the shared TypeScript modules at `../../shared-libs/ts-modules` and the SDK
base from `../start-sdk`. The frontend talks to
the backend exclusively over JSON-RPC, with reactive state via Patch-DB.

`web/patchdb-ui-seed.json` / `patchdb-ui-seed.beta.json` seed initial UI state
and are embedded into `startbox`.

## Container runtime

`container-runtime/` is a Node.js program that runs inside every service LXC. It
loads the service's JavaScript from its `.s9pk`, manages subcontainers, and
talks to the host daemon over a Unix-socket JSON-RPC channel. It depends on the
**built** SDK at `../start-sdk/dist`. It has its own
[README](container-runtime/README.md), [ARCHITECTURE](container-runtime/ARCHITECTURE.md),
and [AGENTS](container-runtime/AGENTS.md) — read those before editing it.

## Backup filesystem (`backup-fs/`)

`backup-fs/` is the `startos-backup-fs` crate (a workspace member): an encrypted,
erasure-coded FUSE filesystem used for StartOS backups. It builds to the
`startos-backup-fs` binary (installed as `/usr/bin/startos-backup-fs` and
`mount.backup-fs`). It was migrated into the monorepo from the former
`Start9Labs/start-fs` repo and is no longer an external git dependency.

## Systemd units and cgroups

- `startd.service` — the main daemon (`Restart=always`, OOM-protected with
  `ManagedOOMPreference=avoid`).
- `services.slice` — the cgroup slice every service container lives under;
  `Delegate=yes` hands the subtree to LXC, and systemd-oomd kills the heaviest
  container under memory pressure rather than wedging the host. `startd` applies
  a RAM-dependent `MemoryMax`/`MemoryHigh` to this slice at boot.
- `startos-shutdown.service` — graceful teardown on power-off only (ties to
  `poweroff.target`/`halt.target`, not reboot); its `ExecStop` calls
  `start-cli server shutdown`.
- `startos-restart.service` — restart handling.

## OS image packaging

Image build inputs are partly shared at the repo root: `debian/` (shared
`debian/build.sh`, `debian/publish.sh`), `apt/`, and `build/` (`env` scripts and
shared build infra). Product-specific maintainer scripts (e.g.
`projects/start-os/debian/postinst`) and `assets/` live in this product dir
(`projects/start-os/`). The root `Makefile`
turns the compiled bins + UIs + runtime squashfs into a `.deb`, `.squashfs`, and
finally the `.iso`/`.img` for the target platform (x86_64, aarch64/Raspberry Pi,
rockchip, riscv64).

## Build pipeline (cross-layer)

Changes flow in one direction; verify in this order:

```
start-core (Rust)
  → make start-core-ts-bindings   # cargo test exports ts-rs types → shared-libs/crates/start-core/bindings/
    → @start9labs/start-core build  # shared-libs/ts-modules/start-core/dist
      → web apps consume @start9labs/start-core
    → start-sdk build  # projects/start-sdk/dist, bundling @start9labs/start-core
      → container-runtime consumes start-sdk/dist (+ @start9labs/start-core)
```

| Step | Command (from repo root)                                  | What it does                                                                |
| ---- | --------------------------------------------------------- | --------------------------------------------------------------------------- |
| 1    | `cargo check -p start-os`                                 | Verify the OS bins compile                                                  |
| 2    | `make start-core-ts-bindings`                             | Export ts-rs types from `start-core`                                        |
| 3    | `cd projects/start-sdk && make bundle`                    | Build the SDK `dist` (builds `@start9labs/start-core` first and bundles it) |
| 4    | `npm run check:ui && npm run check:setup`                 | Type-check the apps                                                         |
| 5    | `cd projects/start-os/container-runtime && npm run check` | Type-check the runtime                                                      |

Editing the generated bindings under `shared-libs/ts-modules/start-core/lib/osBindings/*.ts` alone
is **not** enough — start-core (and the SDK bundle) must be rebuilt before the web apps and
container-runtime see the change.

## Data flow: backend → frontend

StartOS uses Patch-DB (`../../shared-libs/crates/patch-db`) for reactive sync:

1. The backend mutates state via `db.mutate()`, producing CBOR diffs.
2. Diffs are pushed to the frontend over a persistent WebSocket.
3. The frontend applies the diff to its local copy and notifies observers.
4. Components watch specific DB paths and update reactively — no polling.

After a mutating RPC call, the frontend waits for the corresponding diff before
resolving, so the UI is always eventually consistent with the backend.

## Further reading

- [../../shared-libs/crates/start-core](../../shared-libs/crates/start-core) — Rust backend
- [../../shared-libs/ts-modules](../../shared-libs/ts-modules) — shared TypeScript modules + workspace (currently the Angular libs `shared` and `marketplace`)
- [container-runtime/ARCHITECTURE.md](container-runtime/ARCHITECTURE.md) — runtime
