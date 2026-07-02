# AGENTS.md

Rust workspace for StartWRT (crates `ctrl`/`startwrt-core`, `uciedit`, `uciedit_macros`).
Assumes you've read the parent [`../AGENTS.md`](../AGENTS.md) — note especially that these
crates are members of the **root** Cargo workspace (build with `cargo build -p startwrt-core
--bin startwrt` from the repo root; the binary lands in the workspace-root `target/`), and that
`start-core` is pulled in aliased as `startos`.

## Tests

- Run from the repo root, **always `-p`-scoped**:
  `cargo test -p startwrt-core -p uciedit -p uciedit_macros` (or `make test-startwrt` to run the
  same set inside the `start9/cargo-zigbuild` container, mirroring `test-core`).
- **Footgun:** a bare `cargo test` — including running it from `backend/` (which no longer has its
  own workspace `Cargo.toml`) — tests the **entire** monorepo and tries to build
  `startos-backup-fs`→`fuser`, whose build script fails on a host lacking FUSE dev libs. start-wrt's
  own crates are fuser-free, so scope with `-p`.
- Coverage is mostly in `startwrt-core` (~430 inline `#[tokio::test]`/`#[test]`), with the focused
  UCI parser suite in `uciedit/src/tests.rs`.

## Operating rules

- New handler modules must export `pub fn <name><C: CtrlContext>() -> ParentHandler<C>` and be registered in `main_api()` in `ctrl/src/lib.rs`. Skipping the registration is silent — the endpoint won't exist.
- After UCI writes, call `/etc/init.d/<service> reload` — but only when `ctx.effectful()` is true. Reloading unconditionally breaks `--configs-only` CLI mode.
- Use `uciedit`'s retry mechanism for writes that may conflict with concurrent writes (profile creation already retries 4 times). Don't add ad-hoc retry loops.
- The generic `uci.get` / `uci.set` / `file.*` / `exec` endpoints are vestigial — every feature has a purpose-built smart endpoint and no frontend code calls the generics. Don't reach for them when adding new functionality; add a typed handler instead. See `../API_CONTRACT.md` for the full contract.
- For dev authentication, set `STARTWRT_DEV_PASSWORD` to bypass `/etc/shadow`. Don't try to populate `/etc/shadow` on a dev machine.
- Cross-frontend changes: when adding/modifying a handler, update `web/src/app/services/api/api.service.ts` and both `live-api.service.ts` / `mock-api.service.ts`, plus `../API_CONTRACT.md`.
