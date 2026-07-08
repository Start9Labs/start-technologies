# Contributing — start-registry

`start-registry` is a thin product wrapper in the `start-os` monorepo. The registry server/CLI implementation lives in the shared `start-core` crate (`shared-libs/crates/start-core/src/registry/` and `src/bins/registry.rs`), so most changes land there, not in this directory.

Start with the root [`CONTRIBUTING.md`](../../CONTRIBUTING.md) and [`AGENTS.md`](../../AGENTS.md) for repo-wide workflow, the doc map, and cross-layer verification rules. This file covers what's specific to the registry.

## Documentation

- [`README.md`](./README.md) — what the registry is and how to run it.
- [`ARCHITECTURE.md`](./ARCHITECTURE.md) — how it's wired (binary, server, routing, data model).
- `CONTRIBUTING.md` — this file: how to build, test, and contribute.
- [`AGENTS.md`](./AGENTS.md) — rules for agents working in this dir; `CLAUDE.md` is a one-line `@AGENTS.md` import.

## Where to make changes

- **Server/CLI entry, RPC API, data model, persistence, migrations** → `shared-libs/crates/start-core/src/registry/` and `shared-libs/crates/start-core/src/bins/registry.rs`.
- **Bin wiring, systemd unit, docs** → this directory (`projects/start-registry/`).
- **Browsing/search/download UI** → `shared-libs/ts-modules/marketplace/` (`@start9labs/marketplace`).

If you find yourself adding registry logic directly to `projects/start-registry/src`, it almost certainly belongs in `start-core` instead.

## Building

From the monorepo root:

```bash
cargo build -p start-registry --bin registrybox   # build the wrapper bin
cargo check -p start-core                          # type-check the real logic
make start-registry                                       # release musl build
```

Run a local server while iterating:

```bash
./target/debug/registrybox start-registryd --listen 127.0.0.1:5959 --datadir ./registry-data
./target/debug/registrybox start-registry --help
```

## Testing

From the monorepo root:

```bash
cargo test  -p start-core registry                 # registry tests
cargo clippy -p start-core                         # lints
```

Run the checks that apply to what you touched before opening a PR.

## Formatting

```bash
make start-registry-format                               # format (run before committing)
make start-registry-format-check                         # read-only check (what CI runs)
```

Make sure `make start-registry-format-check` is clean before opening a PR.

## Conventions

- **Rust 2024 edition**, formatted via `make start-registry-format` (rustfmt). Keep clippy clean.
- **Comments:** default to none; clear names over prose. A comment is for a non-obvious _why_ only — one short line.
- **API additions:** add subcommands in `registry/mod.rs`; use `with_call_remote::<CliContext>()` to expose them to the `start-registry` CLI and `with_about(...)` for help text. Tag admin-only commands with `with_metadata("admin", true)`.
- **Schema changes:** changing `RegistryDatabase` / index types requires a migration in `shared-libs/crates/start-core/src/registry/migrations`.
- **Version:** `Cargo.toml` `version` (with `# VERSION_BUMP`) tracks the OS release line — don't bump it independently.

## Docs are part of the change

Update the matching docs in the same PR:

- This dir's `README.md` / `ARCHITECTURE.md` for behavior, flags, or structure changes.
- `CHANGELOG.md` (Keep a Changelog style) for any user-visible change.
- OS-level packaging/registry docs in `projects/start-os/docs`, and the marketplace UI, if you change the API contract or the install/run flow.
