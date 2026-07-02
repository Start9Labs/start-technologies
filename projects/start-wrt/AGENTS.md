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
- **`openwrt/` is the monorepo's only git submodule** (a multi-GB external fork). Clone with `--recursive`; the binary build does *not* need it, only the full image does. Don't edit files inside it — changes belong upstream.
- **Build targets live in [`build.mk`](build.mk)** (included by the root `Makefile`), not a standalone product Makefile. From the repo root: `make startwrt` (binary+web), `make startwrt-image` (full image), `make startwrt-update STARTWRT_REMOTE=…` (deploy). When you change a build input in `build.mk`, mirror it into `.github/workflows/start-wrt.yaml` `paths:` (root AGENTS.md "Coupled changes").

> **UNVALIDATED:** the riscv dockerized cross-build and the OpenWrt image assembly have not
> been run since the migration. `cargo check`/host build of the backend passes; the
> `make startwrt` cross-build and `make startwrt-image` still need a build-host run. Don't
> present them as known-good until validated.

## Operating rules

- Don't run `make startwrt-image` (full OpenWrt build) unsolicited — it pulls the openwrt submodule and takes hours. For backend work use `cargo build -p startwrt-core --bin startwrt`; for frontend work use `npm run start:wrt`. Use `make startwrt-update STARTWRT_REMOTE=…` only when explicitly asked to deploy.
- Read the component-level `AGENTS.md` before operating on that component — they document footguns specific to each tree.
- Cross-frontend/backend changes: update `API_CONTRACT.md`, the Rust handler, `web/src/app/services/api/api.service.ts`, and **both** `live-api.service.ts` and `mock-api.service.ts` together. Skipping any breaks the contract.

## Sub-scopes

- [`backend/AGENTS.md`](backend/AGENTS.md) — Rust workspace (ctrl, uciedit, uciedit_macros)
- [`web/AGENTS.md`](web/AGENTS.md) — Angular + Taiga UI frontend
