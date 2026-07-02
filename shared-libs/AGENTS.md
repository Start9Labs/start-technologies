# AGENTS.md — shared-libs

This directory is a container for two cross-product libraries. There is no build
or test target for `shared-libs/` itself — operate inside the relevant sub-library and
read its own `AGENTS.md` first. `CLAUDE.md` is a one-line `@AGENTS.md` import. See
[ARCHITECTURE.md](ARCHITECTURE.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- `crates/start-core/` — Rust backend lib (`start-core`, lib name `start_core`).
  Has its own `AGENTS.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, plus topic notes
  (`core-rust-patterns.md`, `i18n-patterns.md`, `patchdb.md`, `rpc-toolkit.md`,
  `s9pk-structure.md`, `exver.md`, `VERSION_BUMP.md`).
- `ts-modules/` — shared TypeScript modules; the `@start9labs/shared`
  and `@start9labs/marketplace` Angular libraries plus the non-Angular
  `@start9labs/start-core` (`start-core/` — the SDK's core types/ABI/effects/OS
  bindings, consumed by web and bundled into the SDK; the workspace root is the
  repo root, where `angular.json` lives). Has its own `AGENTS.md` and
  `ARCHITECTURE.md` (Taiga UI 5 rules, component conventions, i18n).

## Build & test (run from the repo root)

Rust (`crates/start-core`) — runs from the root Cargo workspace:

```bash
cargo build -p start-core
cargo check -p start-core            # linux-only locally; CI also builds darwin + musl targets
cd shared-libs/crates/start-core && ./run-tests.sh
make format-core                     # format the shared Rust crates (rustfmt); make format-check-core in CI
```

Web (`ts-modules/`) — runs from the repo root (the Angular workspace root, where
`package.json` lives; there is no `package.json` under `ts-modules/`):

```bash
npm ci
npm run build:deps                   # builds @start9labs/start-core + patch-db client (required before typecheck/build)
npm run check                        # typechecks i18n, shared, marketplace, ui, setup, brochure
make format-check-web                # prettier check across the Angular workspace (make format-web to write)
```

## Gotchas

- **No code lives directly in `shared-libs/`** — only the two sub-dirs. Don't add
  files at this level beyond these doc files.
- **start-core is one crate in one workspace.** Build it by package name
  (`-p start-core`), not by `cd`-ing and running a bare `cargo build`. There is a
  single root `Cargo.toml` / `Cargo.lock`.
- **Cross-platform matters for Rust.** Local `cargo check` is linux-only; CI
  builds an apple-darwin + linux-musl matrix. Changes touching `libc`/platform
  APIs or deps can break darwin even when linux passes — cfg-gate
  platform-specific code rather than reimplementing it cross-platform.
- **The repo root is the Angular workspace for ALL front ends.** The product apps
  (`projects/start-os/web/{ui,setup-wizard}`, `projects/start-tunnel/web`,
  `projects/start-wrt/web`, `projects/brochure-marketplace`) build through this root workspace; their
  `angular.json` entries point into the product dirs. Editing
  a shared lib affects every app — run `npm run check` (all projects) after.
- **`build:deps` is a prerequisite.** `@start9labs/start-core` resolves to
  `shared-libs/ts-modules/start-core/dist` and `patch-db-client` to `shared-libs/crates/patch-db/client`
  (from the workspace root); both must be built before typecheck/build will succeed.
- **patch-db is a first-party crate** at repo-root `shared-libs/crates/patch-db/` (not the old
  root `patch-db/`). start-core consumes its Rust `core`; web consumes its
  TS `client`.
- **Web UI work is Taiga UI 5 first** — see `ts-modules/AGENTS.md`; don't hand-roll
  HTML/CSS or guess Taiga APIs.
- Do not edit `CLAUDE.md` files — they are one-line `@AGENTS.md` imports.
