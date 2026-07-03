# Contributing to shared-libs

`shared-libs/` groups two cross-product libraries. Contribute inside the relevant
sub-library; each has its own `CONTRIBUTING.md` with the full detail.

## Documentation

- `README.md` — what this directory is and how to use it.
- [ARCHITECTURE.md](ARCHITECTURE.md) — how the two sub-libs fit into the monorepo.
- [CONTRIBUTING.md](CONTRIBUTING.md) — this file; build/test/format workflow.
- `AGENTS.md` — agent rules; `CLAUDE.md` is a one-line `@AGENTS.md` import.

## Prerequisites

Start from the root [`CONTRIBUTING.md`](../CONTRIBUTING.md) for the shared
toolchain (Rust, Node, Make) and the overall workflow.

## crates/start-core (Rust)

Part of the single root Cargo workspace.

```bash
# from the repo root
cargo build -p start-core
cargo check -p start-core
make start-core-format                        # format the shared Rust crates (rustfmt); make start-core-format-check in CI
cd shared-libs/crates/start-core && ./run-tests.sh
```

- Build by package name (`-p start-core`), never with a bare `cargo build` in the
  crate dir — there is one root `Cargo.toml` / `Cargo.lock`.
- Local `cargo check` is linux-only. CI builds an apple-darwin + linux-musl
  matrix; consider those targets for any change touching `libc`/platform APIs or
  dependencies (cfg-gate platform code rather than reimplementing it).
- See [`crates/start-core/CONTRIBUTING.md`](crates/start-core/CONTRIBUTING.md)
  and the topic notes (`core-rust-patterns.md`, `patchdb.md`, `rpc-toolkit.md`,
  `i18n-patterns.md`, `VERSION_BUMP.md`).

## ts-modules (shared TypeScript modules)

Shared TypeScript modules; the current contents are Angular libraries, built
through the single Angular workspace rooted at the repo root.

```bash
# from the repo root
npm ci
npm run build:deps                      # build @start9labs/start-core + patch-db client (required first)
npm run check                           # typecheck i18n, shared, marketplace, ui, setup, brochure
make web-format                         # prettier --write across the Angular workspace
make web-format-check                   # prettier check (CI)
```

- `build:deps` must run before any typecheck/build: `@start9labs/start-core`
  resolves to `shared-libs/ts-modules/start-core/dist` and `patch-db-client` to
  `shared-libs/crates/patch-db/client`.
- Changes to `ts-modules/shared` or `ts-modules/marketplace` affect every app — run the full
  `npm run check` (it covers all projects) before opening a PR.
- Web UI work follows Taiga UI 5 conventions and mandatory i18n; see
  [`ts-modules/CONTRIBUTING.md`](ts-modules/CONTRIBUTING.md) and `ts-modules/AGENTS.md`.
