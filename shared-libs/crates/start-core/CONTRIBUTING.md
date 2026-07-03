# Contributing to start-core

The shared Rust backend lib (`start-core`, lib name `start_core`) at `shared-libs/crates/start-core`.
For general environment setup, cloning, and the monorepo build system, see the repo-root
[CONTRIBUTING.md](../../../CONTRIBUTING.md).

## Documentation

This sub-tree's docs split across four files:

- `README.md` — what this is
- `ARCHITECTURE.md` — how it's built
- `CONTRIBUTING.md` — this file; how to contribute
- `AGENTS.md` — AI/dev operating rules (`CLAUDE.md` is a one-line `@AGENTS.md` import; don't edit it)

**These docs must be kept up to date.** When you change project structure, conventions, build process, or product context, update the relevant file(s) in the same change — do not defer.

## Prerequisites

- [Rust](https://rustup.rs) (nightly for formatting)
- [rust-analyzer](https://rust-analyzer.github.io/) recommended
- [Docker](https://docs.docker.com/get-docker/) (for cross-compilation via `rust-zig-builder` container)

## Building

Run from the repo root:

```bash
cargo check -p start-core                          # Type check
cargo build -p start-os --bin startbox             # Build a product binary
```

## Testing

Run from the repo root:

```bash
make start-core-test                                     # Run the full suite (wraps run-tests.sh)
cargo test -p start-core <test_name> --features=test  # Run a specific test
```

## Formatting

Run from the repo root:

```bash
make start-core-format                                   # Format with nightly rustfmt
make start-core-format-check                             # Read-only check (CI)
```

## Adding a New RPC Endpoint

1. Define a params struct with `#[derive(Deserialize, Serialize)]`
2. Choose a handler type (`from_fn_async` for most cases)
3. Write the handler function: `async fn my_handler(ctx: RpcContext, params: MyParams) -> Result<MyResponse, Error>`
4. Register it in the appropriate `ParentHandler` tree
5. If params/response should be available in TypeScript, add `#[derive(TS)]` and `#[ts(export)]`

See [rpc-toolkit.md](rpc-toolkit.md) for full handler patterns and all four handler types.

## Adding TS-Exported Types

When a Rust type needs to be available in TypeScript (for the web frontend or SDK):

1. Add `ts_rs::TS` to the derive list and `#[ts(export)]` to the struct/enum
2. Use `#[serde(rename_all = "camelCase")]` for JS-friendly field names
3. For types that don't implement TS (like `DateTime<Utc>`, `exver::Version`), use `#[ts(type = "string")]` overrides
4. For `u64` fields that should be JS `number` (not `bigint`), use `#[ts(type = "number")]`
5. Run `make start-core-ts-bindings` to regenerate — files appear in `shared-libs/crates/start-core/bindings/` then sync to `shared-libs/ts-modules/start-core/lib/osBindings/`
6. Rebuild the affected TS lib(s): `cd shared-libs/ts-modules/start-core && make dist` (web) and/or `cd projects/start-sdk && make bundle` (the SDK bundle for container-runtime)

## Adding i18n Keys

1. Add the key to `locales/i18n.yaml` (i.e. `shared-libs/crates/start-core/locales/i18n.yaml`) with all 5 language translations
2. Use the `t!("your.key.name")` macro in Rust code
3. Follow existing namespace conventions — match the module path where the key is used
4. Use kebab-case for multi-word segments
5. Translations are validated at compile time

See [i18n-patterns.md](i18n-patterns.md) for full conventions.
