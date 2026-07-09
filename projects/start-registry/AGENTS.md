# AGENTS.md — start-registry

Product wrapper for the **Start Registry** server inside the `start-os` monorepo. Crate `start-registry`, multi-call binary `registrybox`. Almost all logic lives in `start-core` — this dir is just the bin entry point, the systemd unit, and docs.

CLAUDE.md is a one-line `@AGENTS.md` import. See [ARCHITECTURE.md](./ARCHITECTURE.md) and [CONTRIBUTING.md](./CONTRIBUTING.md).

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- `src/main.rs` — `MultiExecutable` wiring (`enable_start_registry` + `enable_start_registryd`).
- `Cargo.toml` — crate metadata; `[[bin]] name = "registrybox"`; depends on `start-core = { path = "../../shared-libs/crates/start-core" }`.
- `start-registryd.service` — systemd unit (`ExecStart=/usr/bin/start-registryd`).
- `build/build-registrybox.sh` — release musl build via the `rust-zig-builder` container (what `make start-registry` runs).
- **Real implementation:** `shared-libs/crates/start-core/src/bins/registry.rs` (server `main` + CLI `cli`) and `shared-libs/crates/start-core/src/registry/` (context, db, info, os, package, admin, metrics, signer, migrations, asset).
- **UI:** `shared-libs/ts-modules/marketplace/` (`@start9labs/marketplace`) — not built or served by this crate.

## Build & test (run from the monorepo root, not this dir)

| Command                                           | What                                                                        |
| ------------------------------------------------- | --------------------------------------------------------------------------- |
| `cargo build -p start-registry --bin registrybox` | build the binary (host target, debug)                                       |
| `cargo check -p start-registry`                   | fast type-check (linux only locally)                                        |
| `cargo clippy -p start-registry`                  | lints                                                                       |
| `cargo test -p start-core registry`               | exercise the registry logic (lives in start-core)                           |
| `make start-registry-format`                      | format (`make start-registry-format-check` for the read-only CI check)      |
| `make start-registry`                             | release musl build via `projects/start-registry/build/build-registrybox.sh` |
| `make start-registry-install DESTDIR=…`           | stage binary + symlinks + service                                           |

Because the code lives in `start-core`, most meaningful tests and lints target `-p start-core`, not `-p start-registry`. The thin wrapper mainly verifies that the bin links.

Feature flags are forwarded to `start-core`: `beta`, `console`, `dev`, `test`, `unstable` (e.g. `--features beta`). The release build derives features from the `ENVIRONMENT` build var.

## Gotchas

- **Don't put logic here.** New registry behavior belongs in `shared-libs/crates/start-core/src/registry/`. This crate should stay a wrapper.
- **`registrybox` (bin) vs `start-registry`/`start-registryd` (install names).** The Cargo bin is `registrybox`; the runtime names are symlinks created at install time. The service file references `/usr/bin/start-registryd`, which only exists after `start-registry-install` symlinks it.
- **Version:** `start-registry` is versioned **independently** in `Cargo.toml` (currently `1.0.0`), no longer tied to the StartOS release line. The `.deb` version and `basename.sh` read it straight from the manifest; bump it on its own cadence.
- **`registry_api` is shared by server and CLI.** Adding a subcommand in `registry/mod.rs` with `with_call_remote::<CliContext>()` exposes it both over RPC and through the `start-registry` CLI.
- **Auth:** the RPC route uses local + signature auth; admin-only commands (e.g. metrics) are tagged `with_metadata("admin", true)`.
- **Cross-platform builds.** CI builds musl targets for x86_64/aarch64/riscv64; local `cargo check` is host-only, so platform-specific breaks can slip through (verify against `start-core` CI when touching deps/platform APIs).
- **Manpages:** `cargo test export_manpage_start_registry` (in `start-core`'s `bins/registry.rs`) regenerates this project's committed `man/` pages (`projects/start-registry/man/`).

## Multi-call binary

`registrybox` dispatches on `argv[0]`:

- `start-registryd` → server daemon (`registry::main`)
- `start-registry` → CLI client (`registry::cli`)

Install (root `Makefile`) copies the binary to `/usr/bin/start-registrybox` and symlinks both names to it. When testing locally, pass the role as the first arg: `registrybox start-registryd …` / `registrybox start-registry …`.

## Running locally

```bash
cargo build -p start-registry --bin registrybox
./target/debug/registrybox start-registryd --listen 127.0.0.1:5959 --datadir ./registry-data
./target/debug/registrybox start-registry --help
```

Defaults: listen `127.0.0.1:5959`, datadir `/var/lib/startos` (state in `<datadir>/registry.db` + a SQLite metrics DB + hosted assets). Useful flags: `-l/--listen`, `-H/--hostname`, `-p/--tor-proxy`, `-d/--datadir`, `-c/--config`.

## Docs to update with changes

If you change a flag, the API surface, the data model, or the install/run flow, update this dir's `README.md` / `ARCHITECTURE.md`, add a `CHANGELOG.md` entry, and check the OS-level packaging docs (`projects/start-os/docs/`) plus the marketplace UI if the API contract changed.
