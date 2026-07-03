# AGENTS.md — StartOS OS product

Operating rules for AI developers working in `start-os/`. `CLAUDE.md` is a
one-line `@AGENTS.md` import. See the root [AGENTS.md](../../AGENTS.md) for
monorepo-wide rules, and [ARCHITECTURE.md](ARCHITECTURE.md) and
[CONTRIBUTING.md](CONTRIBUTING.md) for how this product is wired and built.

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- `src/bin/startbox.rs`, `src/bin/start-container.rs` — the only Rust in this
  dir. They are thin entry points; backend logic lives in
  `../../shared-libs/crates/start-core` (crate `start-core`, lib `start_core`).
- `web/ui`, `web/setup-wizard` — Angular apps; part of the workspace at
  `../../shared-libs/ts-modules`. Run web commands from `../../shared-libs/ts-modules`, not from here.
- `container-runtime/` — Node.js LXC runtime with its **own** AGENTS/CLAUDE;
  read `container-runtime/AGENTS.md` before touching it.
- `docs/` — the end-user mdbook (book "StartOS"), served at `/start-os/`.
- Systemd units + `services.slice` live directly in this dir; OS image
  packaging (`debian/`, `apt/`, `build/`) is at the **repo root**; `assets/` is
  directly in this dir.

## Build & test (run from the repo root)

- Compile the OS bins: `cargo check -p start-os` (or `cargo build -p start-os
  --bin startbox`). Local `cargo check` is **linux-only** — CI also builds
  apple-darwin and aarch64/riscv64 musl; platform-specific changes can pass here
  yet break those.
- Regenerate TS bindings after any change to exported Rust types:
  `make start-core-ts-bindings`. Then rebuild start-core (`cd shared-libs/ts-modules/start-core && make dist`)
  and the SDK (`cd projects/start-sdk && make bundle`) before web/runtime type-checks —
  editing `shared-libs/ts-modules/start-core/lib/osBindings/*.ts` alone is not enough.
- Type-check web apps: `npm run check:ui && npm run check:setup`.
- Type-check the runtime: `cd projects/start-os/container-runtime && npm run check`.
- Build the UI: `make start-os-ui` (or `make start-os-uis` for ui + setup-wizard).
- Tests: `make test` (Rust + SDK + container-runtime), or `make start-core-test`.
- Format: `make start-os-format` / `make start-os-format-check`.
- Regenerate `start-container` man pages (committed under `man/`):
  `cargo test -p start-core export_manpage_start_container`.

## Gotchas

- **UIs are embedded into `startbox` at compile time** (`include_dir!`), so the
  web build must precede the Rust build — use the `Makefile`, which encodes the
  ordering, rather than running `cargo build` against a stale `web/dist`.
- **`unshare-userns` must stay a multi-call applet**, not a CLI subcommand: it
  calls `unshare(CLONE_NEWUSER)`, which the kernel rejects on a multi-threaded
  process. See the comment in `src/bin/start-container.rs`.
- **Don't normalize style across components.** The container-runtime uses double
  quotes + no semicolons (its own prettier config); the SDK uses single quotes.
- **Don't edit generated binding files** like
  `shared-libs/ts-modules/start-core/lib/osBindings/index.ts` or `projects/start-sdk/s9pk.mk`.
- **Ask before destructive `make` recipes** — `update*`, `reflash`, `wormhole*`,
  image flashing, and `make clean*` consume hours/disk and may touch a live
  device.
- **The `beta` feature swaps the UI seed** (`patchdb-ui-seed.beta.json`) and
  forwards to `start-core`'s `beta` feature — keep both seeds in sync when you
  change seed shape.

## Docs are part of the change

User-facing changes (UI, CLI output/flags, install/setup flow) must update the
matching page under `docs/` in the same change. Keep this AGENTS, README, and
ARCHITECTURE current when you change structure, build steps, or conventions.
