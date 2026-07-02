# AGENTS.md

Agent/developer operating rules for the **start-technologies monorepo root**. This repo is the monorepo for all Start9 products. `CLAUDE.md` is a one-line `@AGENTS.md` import — do not edit it.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the layout and [CONTRIBUTING.md](CONTRIBUTING.md) for the build/test/format workflow.

**Keep these docs current.** Every scope carries `AGENTS.md` / `ARCHITECTURE.md` / `CONTRIBUTING.md` / `README.md` (with `CLAUDE.md` a one-line `@AGENTS.md` import). When a change alters structure, conventions, the build/test/release flow, or product context, update the matching doc(s) in the **same change** — never defer. These docs are **hierarchical**: each scope's docs cover only what is specific to it and must not repeat anything already stated at a higher scope (e.g. commit/PR conventions live only in this root `CONTRIBUTING.md`).

**A product's user docs and changelog ship with the code.** Any change that alters user-visible behavior **must** update that product's user-facing documentation book (its `docs/` directory — e.g. `projects/start-os/docs/`, `projects/start-tunnel/docs/`, `projects/start-sdk/docs/`) in the **same change**, and **must** add a `CHANGELOG.md` entry for that product (a version bump always pairs with its changelog). Don't land code and defer its docs or changelog to a follow-up.

**Read down into what you touch.** When you work in a subdirectory, first read its `AGENTS.md` — and any further nested `AGENTS.md` on the way down to the files you're editing — before changing anything. Each scope's docs assume you've read the scopes above it, so a subdir's `AGENTS.md` adds only its own rules on top of this root.

## Layout

Each product lives under `projects/` as a thin wrapper; the bulk of the code lives in the top-level shared libs (`shared-libs/`).

- `projects/start-os/` — OS product. Rust bins `startbox` + `start-container` (`src/bin/*.rs`), `web/` (Angular UI + setup-wizard), `container-runtime/` (Node LXC service runtime), OS image build (`build/`), Debian control (`debian/`), VM-setup `assets/`, `backup-fs/`, `docs/`, `*.service`.
- `projects/start-cli/` — `start-cli` bin (`src/main.rs`); thin wrapper over `start-core`.
- `projects/start-registry/` — `registrybox` bin; registry server, serves the shared marketplace UI lib.
- `projects/start-tunnel/` — `tunnelbox` bin + `web/` (StartTunnel UI).
- `projects/start-wrt/` — StartWRT, an OpenWrt-based router OS. Rust backend (`startwrt` bin: RPC daemon + CLI, crates `ctrl`/`uciedit`/`uciedit_macros`) building on shared `start-core`; an Angular `web/` UI (a project in the root Angular workspace) embedded into the binary; `openwrt/` git submodule; flashable image for the SpaceMiT K1.
- `projects/start-sdk/` — `@start9labs/start-sdk` (source in `lib/`; imports the shared `@start9labs/start-core` lib and bundles it into its published `dist/`) + `Makefile`/`s9pk.mk` + `docs/` (packaging mdbook).
- `projects/brochure-marketplace/` — public marketplace/landing Angular app (deploys to marketplace.start9.com).
- `projects/start-docs/` — the documentation website (build infra + landing + Bitcoin guides; each product's own book lives in its `docs/`).
- `shared-libs/crates/start-core/` — the **entire** Rust backend lib (package `start-core`, lib name `start_core`). All six bins depend on it. Internally unchanged from the old `core/` crate.
- `shared-libs/ts-modules/` — shared **TypeScript** modules (the common thread is just that they are TS — not Angular-specific). These are the Angular libs `shared/` (`@start9labs/shared`) and `marketplace/` (`@start9labs/marketplace`), plus the non-Angular `start-core/` (`@start9labs/start-core`: SDK core types/ABI/effects/OS bindings, the TS projection of the `start-core` crate, consumed by web and bundled into the SDK; versionless, not published separately). The Angular workspace is rooted at the repo root (`angular.json`/`package.json`). Product apps reference the libs by package name.
- Top level also holds the shared build infra (`build/`, `Makefile`), `apt/`, the shared `debian/build.sh`, `rfcs/` (protocol drafts), and `shared-libs/crates/patch-db/` (first-party crate, consumed by `start-core` and web).

## Build & test (run from the repo root)

- **Use `make` recipes when they exist** rather than re-deriving the underlying commands. The root `Makefile` is a thin orchestrator that `include`s `build/common.mk` (shared vars/macros) and one `<project>/build.mk` per product (`projects/<name>/build.mk`, `shared-libs/*/build.mk`) — run everything from the repo root (`make startos`, `make registry`, etc.); a product's targets live in its `build.mk`. There is no default target — bare `make` prints `help`.
- **Build a single product** with `cargo build -p <crate> --bin <bin>` (bins: `startbox`/`start-container` in package `start-os`; `start-cli`; `registrybox` in `start-registry`; `tunnelbox` in `start-tunnel`; `startwrt` in package `startwrt-core` for `start-wrt`).
- **Tests:** `make test` (all), `make test-core` / `make test-sdk` / `make test-container-runtime` (scoped). A single Rust test: `cd shared-libs/crates/start-core && cargo test <test_name> --features=test`.
- **Format:** `make format` (Rust nightly fmt + web prettier + SDK); CI runs `make format-check`. See [CONTRIBUTING.md](CONTRIBUTING.md) for the full build/test/format workflow.

## Gotchas

- **Polyglot repo.** Per-component gotchas live in component-level `AGENTS.md` files — read the relevant one before operating on that component (see Sub-scopes).
- **Verify cross-layer changes in order.** Rust → ts-bindings → SDK rebuild → web/container-runtime type checks. See [ARCHITECTURE.md](ARCHITECTURE.md#cross-layer-verification). Editing `shared-libs/ts-modules/start-core/lib/osBindings/*.ts` alone is NOT sufficient — start-core (and the SDK bundle, for container-runtime) must be rebuilt before web/container-runtime will see the change.
- **Ask before destructive `make` recipes.** Image flashing, deploy targets (`update*`, `reflash`, `wormhole*`), and `make clean*` consume hours and disk — confirm with the user first.
- **Git submodule.** `projects/start-wrt/openwrt` is the repo's **only** git submodule (a large external OpenWrt fork; everything else is vendored). Clone with `--recursive` or `git submodule update --init projects/start-wrt/openwrt`. Only start-wrt's full *image* build needs it — every other product, and start-wrt's own binary build, does not.
- **Stale-path watch.** Old docs referenced `core/`, `web/`, `sdk/`, `container-runtime/`, `patch-db/` at the repo root, and the products + `shared/` directly at the root. Those are gone — products now live under `projects/`, the shared libs under `shared-libs/`; use the locations above.

## Coupled changes (keep in sync)

Some pairs of files mirror each other by hand — nothing enforces them, so a change to one half is incomplete until you update the other. Update both in the **same** commit:

- **A product's CI `paths:` filter ↔ its `build.mk` prerequisites.** Each `.github/workflows/<product>.yaml` only triggers on the paths that product's build actually depends on. Those `paths:` allowlists are a hand-maintained mirror of the prerequisites in `projects/<product>/build.mk` (the project dir, `shared-libs/**` or the specific crates it pulls in, `Cargo.*`, `build/**`, `debian/**`, the web config for products with a UI, …). When you add or drop a build input in a `build.mk`, update that product's workflow `paths:` (both the `push:` and `pull_request:` blocks) — otherwise CI will silently stop running on changes that affect the build. Affected pairs: `start-cli`, `start-registry`, `start-tunnel`, `start-wrt`, `startos-iso`.
- **The reusable service-package CI ↔ the SDK package-template ↔ the packaging docs.** `.github/workflows/{build,release,tagAndRelease}.yml` (the `workflow_call` CI that external `*-startos` service repos consume) are mirrored by the copies under `projects/start-sdk/docs/package-template/.github/workflows/` and the examples in `projects/start-sdk/docs/src/project-structure.md`. Change the reusable-workflow surface (inputs, action names, file layout) in all three.
- **Adding a product or crate.** A new crate must be added to the root `Cargo.toml` `members`; a new *product* also needs its `projects/<product>/build.mk` `include`d in the root `Makefile`, a path-gated `.github/workflows/<product>.yaml`, and — if it ships a UI — an `angular.json` project plus `package.json` scripts.

Already enforced or checked elsewhere (listed here for completeness; documented at their own scope):

- **Exported Rust types → `make ts-bindings` → SDK rebuild → web/container-runtime.** See [ARCHITECTURE.md](ARCHITECTURE.md#cross-layer-verification); editing `osBindings/*.ts` alone is not enough.
- **User-facing strings ↔ all five locale dictionaries** (`en_US`/`de_DE`/`es_ES`/`fr_FR`/`pl_PL`) — compile-checked for `start-core`; `npm run check:i18n` for the web libs.
- **`patchdb-ui-seed.json` ↔ `patchdb-ui-seed.beta.json`** — keep both seeds in sync (see [`projects/start-os/AGENTS.md`](projects/start-os/AGENTS.md)).
- **A crate's `version` bump ↔ its `CHANGELOG.md`** — versions are read from each manifest; bump the changelog in the same change.
- **User-facing changes ↔ that product's `docs/`** — docs are part of the change (see each product's AGENTS/CONTRIBUTING).

## Sub-scopes

- [`projects/start-os/AGENTS.md`](projects/start-os/AGENTS.md) — OS product
- [`projects/start-os/container-runtime/AGENTS.md`](projects/start-os/container-runtime/AGENTS.md) — Node.js LXC service runtime
- [`projects/start-cli/AGENTS.md`](projects/start-cli/AGENTS.md) — CLI wrapper over `start-core`
- [`projects/start-registry/AGENTS.md`](projects/start-registry/AGENTS.md) — registry server wrapper
- [`projects/start-tunnel/AGENTS.md`](projects/start-tunnel/AGENTS.md) — tunnel server + UI
- [`projects/start-wrt/AGENTS.md`](projects/start-wrt/AGENTS.md) — OpenWrt-based router OS (Rust backend + Angular UI in the root workspace + openwrt submodule)
- [`projects/start-sdk/AGENTS.md`](projects/start-sdk/AGENTS.md) — TypeScript service-packaging SDK (packaging mdbook: [`docs/AGENTS.md`](projects/start-sdk/docs/AGENTS.md))
- [`projects/brochure-marketplace/AGENTS.md`](projects/brochure-marketplace/AGENTS.md) — public marketplace site
- [`projects/start-docs/AGENTS.md`](projects/start-docs/AGENTS.md) — documentation website
- [`shared-libs/AGENTS.md`](shared-libs/AGENTS.md) — shared libs container: [`crates/start-core`](shared-libs/crates/start-core/AGENTS.md) (Rust backend), [`web`](shared-libs/ts-modules/AGENTS.md) (Angular workspace + UI/setup-wizard/shared libs)
- `shared-libs/crates/patch-db/` — first-party crate (maintained in-tree; the standalone `Start9Labs/patch-db` repo is being retired)
