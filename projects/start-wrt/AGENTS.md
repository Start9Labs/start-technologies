# AGENTS.md

Operating rules for **StartWRT** — Start's OpenWrt-based router OS. This scope assumes
you've read the root [`AGENTS.md`](../../AGENTS.md); only start-wrt-specific rules live here.

StartWRT pairs a Rust backend (single `startwrt` binary: RPC daemon + CLI) with an Angular UI
(a project in the root Angular workspace) embedded into that binary, shipped as a flashable
OpenWrt image for the SpaceMiT K1 (BananaPi-F3). See [ARCHITECTURE.md](ARCHITECTURE.md), [CONTRIBUTING.md](CONTRIBUTING.md) (build
workflow), and [API_CONTRACT.md](API_CONTRACT.md) (the RPC contract).

## Monorepo integration (read this first)

start-wrt was migrated from its own repo into this monorepo. Key consequences:

- **Backend crates are members of the root Cargo workspace** (`projects/start-wrt/backend/{ctrl,uciedit,uciedit_macros}`), not a separate workspace. The binary therefore lands in the **workspace-root `target/`**, not `backend/target/`. Build with `cargo build -p startwrt-core --bin startwrt` from the repo root.
- **`startwrt-core` consumes shared code directly:** the old embedded `start-os` submodule is gone — its `core/` crate is now the shared `start-core`, pulled in **aliased** as `startos` (`startos = { package = "start-core", path = "../../../../shared-libs/crates/start-core" }`) so existing `use startos::…` imports resolve unchanged. `rpc-toolkit` and `imbl-value` likewise point at the vendored `shared-libs/crates/` copies, not git/crates.io.
- **The web is a project in the root Angular workspace** (`start-wrt` in the root `angular.json`), sharing the root `package.json`/`node_modules`/`tsconfig.json` like every other app. Build it with `npm run build:wrt`, serve with `npm run start:wrt`, type-check with `npm run check:wrt` (all run from the repo root). It adopts `@start9labs/shared` for its common utilities — see [`web/AGENTS.md`](web/AGENTS.md).
- **`openwrt/` is a disposable, gitignored build workspace** (no submodule, no fork, no git repo inside). `make start-wrt-openwrt-setup` rebuilds it from the sha256-pinned upstream release tarball ([`build/openwrt-version`](build/openwrt-version)) and applies the Start9 delta: [`openwrt-patches/`](openwrt-patches/) (modified upstream files, applied with `patch -p1`) + [`openwrt-overlay/`](openwrt-overlay/) (added files, rsynced over the tree). Every setup run **rebuilds the tree** (generated state — `dl/`, `build_dir/`, `feeds/`, `files/`, `.config`, keys — is preserved) — never keep work inside it; change the patch/overlay dirs instead (workflow in [CONTRIBUTING.md](CONTRIBUTING.md#openwrt-tree-pinned-upstream--patches--overlay)). The binary build does _not_ need the workspace, only the full image does.
- **Build targets live in [`build.mk`](build.mk)** (included by the root `Makefile`), not a standalone product Makefile. From the repo root: `make start-wrt` (binary+web), `make start-wrt-image` (full image), `make start-wrt-update STARTWRT_REMOTE=…` (deploy). When you change a build input in `build.mk`, mirror it into `.github/workflows/start-wrt.yaml` `paths:` (root AGENTS.md "Coupled changes").

## Operating rules

- Don't run `make start-wrt-image` (full OpenWrt build) unsolicited — it fetches the OpenWrt tree and takes hours. For backend work use `cargo build -p startwrt-core --bin startwrt`; for frontend work use `npm run start:wrt`. Use `make start-wrt-update STARTWRT_REMOTE=…` only when explicitly asked to deploy.
- Read the component-level `AGENTS.md` before operating on that component — they document footguns specific to each tree.
- Cross-frontend/backend changes: update `API_CONTRACT.md`, the Rust handler, `web/src/app/services/api/api.service.ts`, and **both** `live-api.service.ts` and `mock-api.service.ts` together. Skipping any breaks the contract.

## Sub-scopes

- [`backend/AGENTS.md`](backend/AGENTS.md) — Rust workspace (ctrl, uciedit, uciedit_macros)
- [`web/AGENTS.md`](web/AGENTS.md) — Angular + Taiga UI frontend
