# AGENTS.md — shared-libs

This directory groups shared Rust crates and TypeScript modules. There is no build
or test target for `shared-libs/` itself — operate inside the relevant sub-library and
read its own `AGENTS.md` first. `CLAUDE.md` is a one-line `@AGENTS.md` import. See
[ARCHITECTURE.md](ARCHITECTURE.md) for structure.

## Layout

- `crates/start-core/` — Rust backend lib (`start-core`, lib name `start_core`).
  Has its own `AGENTS.md` and `ARCHITECTURE.md`, plus topic notes
  (`core-rust-patterns.md`, `i18n-patterns.md`, `patchdb.md`, `rpc-toolkit.md`,
  `s9pk-structure.md`, `exver.md`, `VERSION_BUMP.md`).
- `crates/rpc-toolkit/` — JSON-RPC handlers, transports, CLI bindings and optional
  TypeScript method-tree generation.
- `crates/visit-rs/` and `crates/visit-rs-derive/` — generic value/type reflection and
  opaque metadata. RPC owns serde normalization and TypeScript policy. Both start-core and StartWRT use the
  in-workspace runtime crate; rpc-toolkit uses it for its opt-in `ts` feature.
- The other first-party Rust crates have their own scoped guidance.
- `ts-modules/` — shared TypeScript modules; the `@start9labs/shared`
  and `@start9labs/marketplace` Angular libraries plus the non-Angular
  `@start9labs/start-core` (`start-core/` — the SDK's core types/ABI/effects/OS
  bindings, consumed by web and bundled into the SDK; the workspace root is the
  repo root, where `angular.json` lives). Has its own `AGENTS.md` and
  `ARCHITECTURE.md` (structure and data flow).

## Contributor workflow

Build and test Rust by package name from the root workspace. `make start-core-test`
exercises the backend; `make rpc-toolkit-test` exercises reflection and RPC bindings
in their supported feature configurations, including generated-module checks with
the root pinned TypeScript compiler. The latter installs its Node prerequisites
through the existing workspace make rules.

`make start-core-format` and `make start-core-format-check` include every shared
Rust crate. Native Rust builds are Linux-only; CI also verifies the product target
matrix. Platform-specific dependencies and APIs must preserve those targets.

Web (`ts-modules/`) — runs from the repo root (the Angular workspace root, where
`package.json` lives; there is no `package.json` under `ts-modules/`):

```bash
npm ci
npm run build:deps                   # builds @start9labs/start-core + patch-db client (required before typecheck/build)
npm run check                        # typechecks all projects
make web-format-check                # prettier check across the Angular workspace (make web-format to write)
```

## Gotchas

- **No code lives directly in `shared-libs/`** — only the two sub-dirs.
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
- **patch-db is a first-party crate** at repo-root `shared-libs/crates/patch-db/`. start-core consumes its Rust `core`; web consumes its
  TS `client`.
- **Web UI work follows the `start9-frontend` skill** at the repo root
  (`.claude/skills/start9-frontend/`) — see `ts-modules/AGENTS.md`.
- Do not edit `CLAUDE.md` files — they are one-line `@AGENTS.md` imports.

## Shared Rust formatting

Every crate under `crates/` formats through `make start-core-format` and
`make start-core-format-check`, using the root pinned-nightly formatter.
