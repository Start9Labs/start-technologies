# AGENTS.md — StartOS OS product

Operating rules for AI developers working in `start-os/`. `CLAUDE.md` is a
one-line `@AGENTS.md` import. See the root [AGENTS.md](../../AGENTS.md) for
monorepo-wide rules, and [ARCHITECTURE.md](ARCHITECTURE.md) and
the root [CONTRIBUTING.md](../../CONTRIBUTING.md) for shared setup and conventions.

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## Layout

- `src/bin/startbox.rs`, `src/bin/start-container.rs` — the only Rust in this
  dir. They are thin entry points; backend logic lives in
  `../../shared-libs/crates/start-core` (crate `start-core`, lib `start_core`).
- `web/ui`, `web/setup-wizard` — Angular apps in the root Angular workspace
  (`angular.json` at the repo root). Run web commands (`npm run check:ui`, `npm run start:ui`, …)
  from the repo root, not from here.
- `container-runtime/` — Node.js LXC runtime with its **own** AGENTS/CLAUDE;
  read `container-runtime/AGENTS.md` before touching it.
- `docs/` — the end-user mdbook (book "StartOS"), served at `/start-os/`.
- `build/` — OS image assembly (image-recipe, dpkg-deps, firmware) plus the
  `startbox`/`start-container` build scripts; `debian/` — Debian control;
  `backup-fs/` carries its own build script. Systemd units + `services.slice`
  and `assets/` live directly in this dir; the shared build infra (root
  `build/`) and `apt/` are at the repo root.

## Prerequisites

The OS product is a thin wrapper over `shared-libs/crates/start-core`, the shared
TypeScript modules, and the SDK. Build commands run from the repo root unless
noted otherwise. Start with the root `CONTRIBUTING.md` for the shared Rust,
Node, Docker, Make, and git toolchain.

Building a bootable OS image additionally needs multi-architecture emulation
and image-packaging tools on Debian or Ubuntu:

```sh
sudo apt install -y qemu-user-static binfmt-support squashfs-tools b3sum
docker run --privileged --rm tonistiigi/binfmt --install all
docker buildx create --name start9 --use 2>/dev/null || docker buildx use start9
```

Web-only work does not need the image toolchain. For faster local iteration,
source `projects/start-os/devmode.sh` from the repo root; it sets
`ENVIRONMENT=dev` and `GIT_BRANCH_AS_HASH=1`.

## Build configuration

StartOS accepts the root build variables `PLATFORM`, `ENVIRONMENT`, `PROFILE`,
and `GIT_BRANCH_AS_HASH`.

- `PLATFORM`: `x86_64`, `x86_64-nonfree`, `aarch64`, `aarch64-nonfree`,
  `riscv64`, or `raspberrypi`. Nonfree variants add proprietary firmware and
  drivers; Raspberry Pi necessarily includes nonfree components. The selected
  platform is remembered between builds.
- `ENVIRONMENT`: hyphen-separated flags: `dev` (password SSH before setup and
  no frontend compression), `unstable` (assertions/debugging at a performance
  cost), and `console` (tokio-console).

## Build & test (run from the repo root)

- Compile the OS bins: `cargo check -p start-os` (or
  `cargo build -p start-os --bin startbox`). Local `cargo check` is
  **linux-only** — CI also builds
  apple-darwin and aarch64/riscv64 musl; platform-specific changes can pass here
  yet break those.
- Regenerate TS bindings after any change to exported Rust types:
  `make start-core-ts-bindings`. Then rebuild start-core (`cd shared-libs/ts-modules/start-core && make dist`)
  and the SDK (`cd projects/start-sdk && make bundle`) before web/runtime type-checks —
  editing `shared-libs/ts-modules/start-core/lib/osBindings/*.ts` alone is not enough.
- Type-check web apps: `npm run check:ui && npm run check:setup`.
- Type-check the runtime: `cd projects/start-os/container-runtime && npm run check`.
- Build the UI: `make start-os-ui` (or `make start-os-uis` for ui + setup-wizard).
- Tests: `make test` (Rust + SDK + container-runtime), or `make start-core-test`.
- Format: `make start-os-format` / `make start-os-format-check` (Rust only);
  TS/web/container-runtime formatting runs through `make web-format` (root
  prettier config).
- Regenerate `start-container` man pages (committed under `man/`):
  `cargo test -p start-core export_manpage_start_container`.

Primary product/image targets:

```sh
make start-os                   # bins + UI + container-runtime image
make start-os-ui                # admin UI (start-os-uis also builds setup)
make start-os-$(IMAGE_TYPE)     # bootable image: start-os-iso or start-os-img
make start-os-deb               # Debian package
make start-os-squashfs          # squashfs image
```

## Deploying to a device

These targets modify a live device and are slow or destructive. Ask the user
before running any of them.

| Target                                        | Purpose                               |
| --------------------------------------------- | ------------------------------------- |
| `start-os-update-startbox REMOTE=start9@<ip>` | Deploy binary + UI only               |
| `start-os-update-deb REMOTE=start9@<ip>`      | Deploy the Debian package             |
| `start-os-update REMOTE=start9@<ip>`          | OTA-style update                      |
| `start-os-emulate-reflash REMOTE=start9@<ip>` | Reflash like a live ISO               |
| `start-os-update-overlay REMOTE=start9@<ip>`  | Deploy to the reboot-volatile overlay |
| `start-os-wormhole`                           | Send the startbox binary remotely     |
| `start-os-wormhole-deb`                       | Send the Debian package remotely      |
| `start-os-wormhole-squashfs`                  | Send the squashfs remotely            |

## Creating a VM

Install `virt-manager`, add the user to `libvirt`, build an ISO with
`PLATFORM=$(uname -m) ENVIRONMENT=dev make start-os-iso`, then follow the
screenshots under `assets/create-vm/`. Point a storage pool at the root
`results/` directory and select a generic/unknown OS. Start a new login session
after adding the user to `libvirt` so the group membership takes effect.

## Community and security

Use the [packaging guide](https://docs.start9.com/packaging) for service-package
work rather than this product workflow. StartOS development discussion is in
the [developer Matrix room](https://matrix.to/#/#dev-startos:matrix.start9labs.com).
Report security issues privately to [security@start9.com](mailto:security@start9.com).

## Cross-layer verification

For Rust types exported to TypeScript, verify in this order:

1. `cargo check -p start-os`
2. `make start-core-ts-bindings`
3. `cd shared-libs/ts-modules/start-core && make dist`
4. `cd projects/start-sdk && make bundle`
5. `npm run check:ui && npm run check:setup`
6. `cd projects/start-os/container-runtime && npm run check`

## Gotchas

- **UIs are embedded into `startbox` at compile time** (`include_dir!`), so the
  web build must precede the Rust build — use the `Makefile`, which encodes the
  ordering, rather than running `cargo build` against a stale `web/dist`.
- **`unshare-userns` must stay a multi-call applet**, not a CLI subcommand: it
  calls `unshare(CLONE_NEWUSER)`, which the kernel rejects on a multi-threaded
  process. See the comment in `src/bin/start-container.rs`.
- **One prettier config.** All TS (web, container-runtime) is governed by the
  root `.prettierrc.json` + `.prettierignore`; run prettier from the repo root
  so the ignore applies (`__fixtures__/` etc. must stay unformatted). Don't add
  per-component prettier configs or scripts.
- **Don't edit generated binding files** like
  `shared-libs/ts-modules/start-core/lib/osBindings/index.ts` or `projects/start-sdk/s9pk.mk`.
- **Ask before destructive `make` recipes** — `update*`, `reflash`, `wormhole*`,
  image flashing, and `make clean*` consume hours/disk and may touch a live
  device.
- **The `beta` feature swaps the UI seed** (`patchdb-ui-seed.beta.json`) and
  forwards to `start-core`'s `beta` feature — keep both seeds in sync when you
  change seed shape.

## Docs are part of the change

User-facing changes (UI, CLI output/flags, install/setup flow) must update the
matching page under `docs/` in the same change. Keep this AGENTS, README, and
ARCHITECTURE current when you change structure, build steps, or conventions.
