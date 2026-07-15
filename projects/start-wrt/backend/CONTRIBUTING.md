# Contributing to Backend

For general setup and build system, see the product-level [CONTRIBUTING.md](../CONTRIBUTING.md). For architecture details, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Documentation

This sub-tree's docs split across five files:

- `README.md` — what this is
- `ARCHITECTURE.md` — how it's built
- `CONTRIBUTING.md` — this file; how to contribute
- `AGENTS.md` — AI-developer operating rules (handler registration, effectful flag, retry, vestigial endpoints)
- `CLAUDE.md` — a one-line `@AGENTS.md` import

**These docs must be kept up to date.** When you change the backend's crate layout, handler conventions, UCI library API, or test surface, update the relevant file(s) in the same change — do not defer.

## Tech Stack

- **Rust** (2021 edition) — 3 crates, members of the root monorepo workspace
- **axum** + **tokio**, served through start-core's `WebServer` (`startos::net::web_server`) for HTTP/HTTPS
- **rpc-toolkit** for JSON-RPC 2.0 (vendored at `shared-libs/crates/rpc-toolkit`)
- **uciedit** for parsing/writing OpenWrt UCI config files (workspace crate)
- **clap** for CLI argument parsing
- **serde** for serialization

## Getting Started

These crates are members of the **root** monorepo Cargo workspace — run from the repo root and
always scope with `-p` (a bare `cargo build`/`cargo test` targets the entire monorepo, which pulls
in `startos-backup-fs`→`fuser` and fails on a bare host without FUSE dev libs):

```bash
cargo build -p startwrt-core --bin startwrt                # Build the daemon+CLI binary
cargo check -p startwrt-core --bin startwrt                # Type-check without building
cargo test  -p startwrt-core -p uciedit -p uciedit_macros  # Run all start-wrt unit tests
make start-wrt-test                                         # same tests, containerized (mirrors start-core-test)
```

Cross-compilation for the router target (riscv64gc-unknown-linux-musl) is handled by `../build/build-rust.sh` (`projects/start-wrt/build/build-rust.sh`).

For dev authentication, set `STARTWRT_DEV_PASSWORD` to bypass `/etc/shadow` validation.

### Crates

| Crate            | Package                          | Description                                                            |
| ---------------- | -------------------------------- | ---------------------------------------------------------------------- |
| `ctrl`           | `startwrt-core` (lib `startwrt`) | RPC server (`startwrt-ctrld`) and CLI (`startwrt-cli`)                 |
| `uciedit`        | `uciedit`                        | UCI config parser/serializer with atomic writes and conflict detection |
| `uciedit_macros` | `uciedit_macros`                 | `#[derive(TypedSection)]` proc macro for typed UCI sections            |

### Other Directories

- `firstboot_config/` — Factory-default UCI configs, copied into the OpenWrt image's `/etc/config/` at image staging time (`../build/stage-files.sh`)
- `config_experiments/` — Reference UCI configs for manual testing

## Adding a New RPC Endpoint

1. **Define param/response types** in your module:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MyParams {
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MyResponse {
    success: bool,
}
```

2. **Write the handler function:**

```rust
pub async fn my_handler<C: CtrlContext>(ctx: C, args: MyParams) -> Result<MyResponse, Error> {
    let arena = Arena::new();
    let mut cfgs = parse_all(ctx.uci_root(), &arena, &["network"]).await?;
    // ... read/modify UCI configs ...
    dump_all(ctx.uci_root(), cfgs).await?;
    if ctx.effectful() {
        run_quiet_async(Command::new("/etc/init.d/network").arg("reload")).await?;
    }
    Ok(MyResponse { success: true })
}
```

3. **Register in the module's parent handler:**

```rust
pub fn my_module<C: CtrlContext + Clone>() -> ParentHandler<C> {
    ParentHandler::new()
        .subcommand("my-method", from_fn_async(my_handler).with_display_serializable())
}
```

4. **Register the module in `main_api()`** in `ctrl/src/lib.rs`:

```rust
.subcommand("my-module", my_module::my_module::<C>())
```

5. **Add to `ApiService`** in the frontend (`web/src/app/services/api/api.service.ts`) and both implementations (`live-api.service.ts`, `mock-api.service.ts`).

6. **Add to [API_CONTRACT.md](../API_CONTRACT.md)** with Rust types.

## Adding a Typed UCI Section

1. **Define the struct** in `uciedit/src/openwrt.rs`:

```rust
#[derive(Debug, TypedSection, Default)]
#[uci(ty = "mytype")]
pub struct MySection {
    pub name: String,
    #[uci(default)]
    pub enabled: bool,
    #[uci(rename = "type")]
    pub kind: String,
}
```

Macro attributes:

- `#[uci(ty = "name")]` — UCI section type
- `#[uci(rename = "option")]` — field name differs from UCI option name
- `#[uci(default)]` — use `Default::default()` if option missing
- `#[uci(default_value = expr)]` — custom default value
- `#[uci(inpt)]` — use `inpt` parser instead of `FromStr`

2. **Use it** in handler code:

```rust
let arena = Arena::new();
let cfg = Config::parse(&arena, ctx.uci_root().join("myconfig")).await?;
cfg.try_each(|section_name, section: MySection| {
    // process each section of this type
    Ok(())
})?;
```

## Testing

Run from the repo root, scoped with `-p` (a bare `cargo test` tests the whole monorepo and trips
`fuser` on a bare host — see Getting Started):

```bash
cargo test -p startwrt-core          # Handler tests — the bulk of coverage (~430 tests)
cargo test -p uciedit                # UCI parser tests
cargo test -p startwrt-core -p uciedit -p uciedit_macros   # everything
make start-wrt-test                   # all of the above, containerized (mirrors start-core-test)
```

`startwrt-core`'s handler tests write fixtures into per-test tempdirs (see
`ctrl/src/lan.rs::setup_fixtures`); UCI parser tests use inline config strings (see
`uciedit/src/tests.rs`).
