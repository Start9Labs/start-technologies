# AGENTS.md

Operating rules for **StartWRT** — Start's OpenWrt-based router OS. This scope assumes
you've read the root [`AGENTS.md`](../../AGENTS.md); only start-wrt-specific rules live here.

StartWRT pairs a Rust backend (single `startwrt` binary: RPC daemon + CLI) with an Angular UI
(a project in the root Angular workspace) embedded into that binary, shipped as a flashable
OpenWrt image for the SpaceMiT K1 (BananaPi-F3). See [ARCHITECTURE.md](ARCHITECTURE.md) and [API_CONTRACT.md](API_CONTRACT.md) (the RPC contract).

## Monorepo integration (read this first)

start-wrt was migrated from its own repo into this monorepo. Key consequences:

- **Backend crates are members of the root Cargo workspace** (`projects/start-wrt/backend/{ctrl,uciedit,uciedit_macros}`), not a separate workspace. The binary therefore lands in the **workspace-root `target/`**, not `backend/target/`. Build with `cargo build -p startwrt-core --bin startwrt` from the repo root.
- **`startwrt-core` consumes shared code directly:** the old embedded `start-os` submodule is gone — its `core/` crate is now the shared `start-core`, pulled in **aliased** as `startos` (`startos = { package = "start-core", path = "../../../../shared-libs/crates/start-core" }`) so existing `use startos::…` imports resolve unchanged. `rpc-toolkit` and `imbl-value` likewise point at the vendored `shared-libs/crates/` copies, not git/crates.io.
- **The web is a project in the root Angular workspace** (`start-wrt` in the root `angular.json`), sharing the root `package.json`/`node_modules`/`tsconfig.json` like every other app. Build it with `npm run build:wrt`, serve with `npm run start:wrt`, type-check with `npm run check:wrt` (all run from the repo root). It adopts `@start9labs/shared` for its common utilities — see [`web/AGENTS.md`](web/AGENTS.md).
- **`openwrt/` is a disposable, gitignored build workspace** (no submodule, no fork, no git repo inside). `make start-wrt-openwrt-setup` rebuilds it from the sha256-pinned upstream release tarball ([`build/openwrt-version`](build/openwrt-version)) and applies the Start9 delta: [`openwrt-patches/`](openwrt-patches/) (modified upstream files, applied with `patch -p1`) + [`openwrt-overlay/`](openwrt-overlay/) (added files, rsynced over the tree). Every setup run **rebuilds the tree** (generated state — `dl/`, `build_dir/`, `feeds/`, `files/`, `.config`, keys — is preserved) — never keep work inside it; change the patch/overlay dirs instead (workflow in [AGENTS.md](AGENTS.md#openwrt-tree-pinned-upstream--patches--overlay)). The binary build does _not_ need the workspace, only the full image does.
- **Build targets live in [`build.mk`](build.mk)** (included by the root `Makefile`), not a standalone product Makefile. From the repo root: `make start-wrt` (binary+web), `make start-wrt-image` (full image), `make start-wrt-update STARTWRT_REMOTE=…` (deploy). When you change a build input in `build.mk`, mirror it into `.github/workflows/start-wrt.yaml` `paths:` (root AGENTS.md "Coupled changes").

## Operating rules

- Don't run `make start-wrt-image` (full OpenWrt build) unsolicited — it fetches the OpenWrt tree and takes hours. For backend work use `cargo build -p startwrt-core --bin startwrt`; for frontend work use `npm run start:wrt`. Use `make start-wrt-update STARTWRT_REMOTE=…` only when explicitly asked to deploy.
- Read the component-level `AGENTS.md` before operating on that component — they document footguns specific to each tree.
- Cross-frontend/backend changes: update `API_CONTRACT.md`, the Rust handler, `web/src/app/services/api/api.service.ts`, and **both** `live-api.service.ts` and `mock-api.service.ts` together. Skipping any breaks the contract.

## Sub-scopes

- [`backend/AGENTS.md`](backend/AGENTS.md) — Rust workspace (ctrl, uciedit, uciedit_macros)
- [`web/AGENTS.md`](web/AGENTS.md) — Angular + Taiga UI frontend

## Contributor workflow

Run commands from the repo root unless a block explicitly changes directory.

## Backend (Rust)

The three crates (`startwrt-core`/`ctrl`, `uciedit`, `uciedit_macros`) are members of the root
Cargo workspace.

```bash
cargo build -p startwrt-core --bin startwrt                     # host build of the daemon+CLI binary
cargo check -p startwrt-core --bin startwrt                     # fast type-check
cargo test  -p startwrt-core -p uciedit -p uciedit_macros       # all start-wrt unit tests
make start-wrt-test                                              # same tests, containerized (mirrors start-core-test)
```

`startwrt-core` depends on the shared `start-core` crate (aliased as `startos`), plus the
vendored `rpc-toolkit` and `imbl-value`. For dev authentication set `STARTWRT_DEV_PASSWORD` to
bypass `/etc/shadow`.

> The host build embeds the web UI via `include_dir!`, so it needs `projects/start-wrt/web/dist/`
> to exist — run the web build first (below), or build the full binary with `make start-wrt`.

## Frontend (Angular, in the root workspace)

The web app is the `start-wrt` project in the root Angular workspace — it shares the root
`package.json`/`node_modules`/`tsconfig.json`. Run everything from the repo root:

```bash
npm ci                  # install the whole workspace
npm run build:deps      # build the file: deps (@start9labs/start-core, patch-db client) — once after install
npm run start:wrt       # dev server (mock API, no backend needed) — stamps config.json first
npm run build:wrt       # production build → projects/start-wrt/web/dist/startwrt/browser/
npm run check:wrt       # type-check
npm run check:i18n:wrt  # i18n dictionary check
```

## Building / deploying (via the root Makefile)

start-wrt's targets live in [`build.mk`](build.mk) (included by the root `Makefile`):

`make start-wrt-rpc-bindings` regenerates the committed RPC and streaming bindings;
`make start-wrt-rpc-bindings-check` regenerates them and rejects drift. Both run the
host generator with Cargo's `bindings` profile (unoptimized, no debug info,
nonincremental). Product builds consume the committed bindings and retain their
selected `PROFILE`.

| Target                                          | Description                                                                        |
| ----------------------------------------------- | ---------------------------------------------------------------------------------- |
| `make start-wrt`                                | web → riscv64 binary (cross-compiled via dockerized cargo-zigbuild)                |
| `make start-wrt-openwrt-setup`                  | fetch/reset the pinned OpenWrt tree, apply the Start9 delta, feeds/config/download |
| `make start-wrt-image`                          | full flashable OpenWrt image → `results/` (**hours**)                              |
| `make start-wrt-update STARTWRT_REMOTE=root@IP` | deploy binary over SSH (default `root@192.168.0.1`)                                |
| `make start-wrt-clean`                          | remove start-wrt build artifacts                                                   |

The OpenWrt image build needs a consistent environment — Docker is recommended; native builds
on some distros fail silently.

Deployment is atomic (temp file → sync → rename → daemon restart). The web UI is embedded in
the binary, so deploying the binary updates everything.

## Cutting a release

StartWRT is a first-class project of the monorepo-wide release tool,
[`scripts/manage-release.sh`](../../scripts/manage-release.sh) (the `wrt` kind). The version is
read from `backend/ctrl/Cargo.toml`; the git tag / GitHub release is `start-wrt/v<version>`.
Releases stage through a beta registry before promotion to production, mirroring the OS.

1. Set the release version in `backend/ctrl/Cargo.toml`, add changelog fragments under the
   root rules, and write `release-notes/<version>.md`. Land them on `master`.
2. Run the **start-wrt** workflow with `deploy: release`. It builds the OpenWrt image, uploads
   the images to `s3://startwrt-images`, and registers + indexes the version into the **beta
   registry** (signing with the `DEV_KEY` repo secret). Beta routers — any router whose UCI
   `startwrt.system.registry` points at the beta registry (`uci set
startwrt.system.registry=<beta url>; uci commit startwrt`) — now soak the version as a
   normal OTA update. (If the register step failed or must be redone, the manual fallback is
   `RUN_ID=<the deploy run> ./scripts/manage-release.sh pull-gha start-wrt` followed by
   `./scripts/manage-release.sh register start-wrt` — needs `gh`, `start-cli`, and
   `~/.startos/developer.key.pem`.)
3. Once the version has soaked, cut the release from the repo root (needs `gh`, `gpg` with the
   Start9 org key, `start-cli`, and `~/.startos/developer.key.pem`):

   ```
   ./scripts/manage-release.sh release start-wrt
   ```

   This runs pre-check → pull the images from the beta registry (signature-verified) → tag →
   create the GitHub release → promote beta → production → sign. See `manage-release.sh --help`
   for the individual subcommands (`pull-gha`, `register`, `index`, `sign`, `cosign`, …) and env
   vars (`STARTWRT_SOURCE_REGISTRY`, `STARTWRT_TARGET_REGISTRY`, `STARTWRT_COMPAT_FLOOR`,
   `FORCE=1` to re-run an idempotent release).

## OpenWrt tree (pinned upstream + patches + overlay)

`openwrt/` is **not** a submodule, **not** a fork, and **not even a git repo** — it's a
disposable, gitignored build workspace (think `node_modules/`) that `build/openwrt-setup.sh`
rebuilds. Every setup run:

1. Downloads the upstream release tarball pinned in
   [`build/openwrt-version`](build/openwrt-version) (`OPENWRT_VERSION`, integrity-checked
   against `OPENWRT_TARBALL_SHA256`; cached at `openwrt/dl/openwrt-v<ver>.tar.gz`) and
   extracts a pristine tree, discarding any local edits. Generated state the tarball doesn't
   provide (`dl/`, `feeds/`, `build_dir/`, `staging_dir/`, `bin/`, `files/`, `.config`,
   signing keys, …) is carried over, so caches and staged files are preserved.
2. Applies [`openwrt-patches/`](openwrt-patches/) with `patch -p1` — the Start9 modifications
   to upstream files.
3. Rsyncs [`openwrt-overlay/`](openwrt-overlay/) over the tree — the _added_ files
   (mirroring upstream layout): `target/linux/spacemit/` (the K1 target, including its
   `patches-6.18/` kernel patches), `package/boot/{opensbi,uboot}-spacemit/`, the generic
   6.18 kernel stubs, and one mac80211 build patch. Additions live as plain files, not
   patches, so upstream bumps can never conflict with them. Most of this tree comes from
   SpaceMiT's BSP and from OpenWrt, and it is GPL-2.0-only rather than MIT — see
   [`openwrt-overlay/README.md`](openwrt-overlay/README.md) before editing it.

`./projects/start-wrt/build/openwrt-setup.sh --tree-only` (from the repo root) runs only the
tree rebuild — useful offline (once the tarball is cached) and for testing.

**Changing the OpenWrt delta.** Never keep work inside `openwrt/` — the next setup run
discards it. To add files, edit `openwrt-overlay/` (or prototype in the workspace, verify,
then copy the files into the overlay at the same relative path). To modify an upstream file,
edit it in the workspace, verify, then regenerate the patch against the pristine copy pulled
straight from the cached tarball:

```bash
cd projects/start-wrt
tar -xzf openwrt/dl/openwrt-v<ver>.tar.gz openwrt-<ver>/<path> -O > /tmp/pristine
diff -u /tmp/pristine openwrt/<path> \
  | sed -e 's|^--- .*|--- a/<path>|' -e 's|^+++ .*|+++ b/<path>|' \
  > openwrt-patches/000N-<name>.patch
```

(Keep the explanatory header block above the diff — patch tooling ignores everything before
the first `---`/`diff` line.)

**Bumping the upstream release.** Update both values in `build/openwrt-version` (the new
version and the sha256 of its tag tarball — download it once and `sha256sum` it), run
`make start-wrt-openwrt-setup`, and rebuild the image. If a patch no longer applies, fix the
affected file in the workspace by hand and regenerate that patch as above; either way, refresh
each patch's `Applies to:` header line to the new version. The overlay needs
attention only if upstream grew a conflicting path (the spacemit target dir is ours alone, so
this is rare). Commit the pin bump + refreshed patches + a changelog fragment as one
ordinary PR.
