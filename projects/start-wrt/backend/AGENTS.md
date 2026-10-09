# AGENTS.md

Rust crates for StartWRT (crates `ctrl`/`startwrt-core`, `uciedit`, `uciedit_macros`).
Assumes you've read the parent [`../AGENTS.md`](../AGENTS.md) — note especially that these
crates are members of the **root** Cargo workspace (build with `cargo build -p startwrt-core
--bin startwrt` from the repo root; the binary lands in the workspace-root `target/`), and that
`start-core` is pulled in aliased as `startos`.

## Tests

- Run from the repo root, **always `-p`-scoped**:
  `cargo test -p startwrt-core -p uciedit -p uciedit_macros` (or `make start-wrt-test` to run the
  same set inside the `start9/cargo-zigbuild` container, mirroring `start-core-test`).
- **Footgun:** a bare `cargo test` from `backend/` selects the root workspace
  rather than only StartWRT. Scope with `-p` to avoid unrelated product builds and
  their prerequisites.
- Coverage is mostly in `startwrt-core` (inline `#[tokio::test]`/`#[test]`), with the focused
  UCI parser suite in `uciedit/src/tests.rs`.

## Operating rules

- New handler modules must export `pub fn <name><C: CtrlContext>() -> ParentHandler<C>` and be registered in `main_api()` in `ctrl/src/lib.rs`. Skipping the registration is silent — the endpoint won't exist.
- After UCI writes, call `/etc/init.d/<service> reload` — but only when `ctx.effectful()` is true. Reloading unconditionally breaks `--configs-only` CLI mode.
- Use `uciedit`'s retry mechanism for writes that may conflict with concurrent writes (profile creation already retries 4 times). Don't add ad-hoc retry loops.
- The generic `uci.get` / `uci.set` / `uci.edit` / `file.*` / `exec` endpoints are vestigial — every feature has a purpose-built smart endpoint and no frontend code calls the generics. Don't reach for them when adding new functionality; add a typed handler instead. See `../API_CONTRACT.md` for the full contract.
- For dev authentication, set `STARTWRT_DEV_PASSWORD` to bypass `/etc/shadow`. Don't try to populate `/etc/shadow` on a dev machine.

## Adding a New RPC Endpoint

1. **Define param/response types** in your module:

```rust
#[derive(Deserialize, clap::Parser, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
struct MyParams {
    name: String,
}
rpc_toolkit::reflect_ts!(MyParams);

#[derive(Serialize, visit_rs::VisitFields)]
#[serde(rename_all = "camelCase")]
struct MyResponse {
    success: bool,
}
rpc_toolkit::reflect_ts!(MyResponse);
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

5. Run `make start-wrt-rpc-bindings`, then update the RPC contract and frontend implementations together as required by the parent `AGENTS.md`.

## Production TypeScript generation

Struct DTOs derive `visit_rs::VisitFields`; enum DTOs derive
`visit_rs::VisitVariants`. Both supply raw facts and static traversal, with adjacent
`rpc_toolkit::reflect_ts!` bridges. RPC owns serde normalization, TypeScript
rendering and raw `visit(ts(...))` hint interpretation. Locally select authoritative
type-valued conversion/wire literals with `visit(type_attributes(...))`; use
explicit `visit(opaque)` for hidden storage. Typed `TS::IS_OPTION` combines with
serde policy, including through aliases and wrappers. Generic bridges retain
payload bounds and inline by default. Read the
[TypeScript guide](../../../shared-libs/crates/rpc-toolkit/docs/typescript.md)
before adding custom wire hints or projections.

`ctrl/examples/generate_rpc_bindings.rs` generates the `Api` tree from
`main_api::<ServerContext>()` into `web/src/app/services/api/bindings.ts` and
near-owner `rpc_toolkit::ts_export!(Type, namespaces = ["events"])` streaming
registrations collected by namespace into `events.ts`. Requests use input shapes; returns and streaming events use output
shapes. Both files are generated build inputs — never hand-edit them. CLI-only
backup callbacks use `no_ts()`; the RPC download/upload endpoints remain typed.

## Adding a Typed UCI Section

1. **Define shared OpenWrt-native sections** in `uciedit/src/openwrt.rs`; define feature-owned sections beside their handlers in `ctrl`:

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
