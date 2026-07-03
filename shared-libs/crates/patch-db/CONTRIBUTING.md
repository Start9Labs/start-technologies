# Contributing

## Prerequisites

- **Rust** — stable toolchain (edition 2018+)
- **Node.js** — v16+ with npm

## Building

Run from the repo root (these crates are members of the root Cargo workspace):

### Rust

```bash
cargo build -p patch-db                  # core crate (also -p json-patch / json-ptr / patch-db-macro / …)
cargo build -p patch-db --features debug # with tracing support
```

### TypeScript client

```bash
cd shared-libs/crates/patch-db/client
npm install
npm run build    # Compiles to dist/
npm run check    # Type-check without emitting
```

## Testing

### Rust

```bash
cargo test -p patch-db     # core crate (uses proptest for property-based testing)
cargo test -p json-ptr     # JSON Pointer crate
cargo test -p json-patch   # JSON Patch crate
```

### TypeScript

```bash
cd shared-libs/crates/patch-db/client
npm run check    # Type-check without emitting
```

## CLI utility

`patch-db-util` provides commands for inspecting and restoring database files:

```bash
# Dump database state as JSON
cargo run -p patch-db-util -- dump path/to/my.db

# Restore database from JSON on stdin
echo '{"count": 42}' | cargo run -p patch-db-util -- from-dump path/to/my.db
```

## Code style

Format from the repo root with `make start-core-format` (these crates are part of the shared Rust workspace); CI runs the read-only `make start-core-format-check`.

### Rust

- Follow standard `rustfmt` conventions
- Use `thiserror` for error types
- Async functions use `tokio`

### TypeScript

- RxJS conventions: suffix observables with `$`

## Making changes

1. **Check [ARCHITECTURE.md](ARCHITECTURE.md)** to understand which crate(s) your change touches
2. **Follow existing patterns** — look at neighboring code before inventing new abstractions
3. **Cross-layer changes** (Rust types that affect the TS client) require updating both sides to keep the wire format compatible:
   - `Revision` and `Dump` types must match between `core/src/patch.rs` and `client/lib/types.ts`
   - Patch operations (add/remove/replace) must match between `json-patch/` and `client/lib/json-patch-lib.ts`
4. **Run tests** before submitting
