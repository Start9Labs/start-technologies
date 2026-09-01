# AGENTS.md — StartOS OS product

Operating rules for AI developers working in `start-os/`. `CLAUDE.md` is a
one-line `@AGENTS.md` import. See the root [AGENTS.md](../../AGENTS.md) for
monorepo-wide rules and [ARCHITECTURE.md](ARCHITECTURE.md) for how this product is wired.

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

## Build & test (run from the repo root)

- Compile the OS bins: `cargo check -p start-os` (or `cargo build -p start-os
--bin startbox`). Local `cargo check` is **linux-only** — CI also builds
  apple-darwin and aarch64/riscv64 musl; platform-specific changes can pass here
  yet break those.
- Regenerate TS bindings after any change to exported Rust types:
  `make start-core-ts-bindings`. Then rebuild start-core (`cd shared-libs/ts-modules/start-core && make dist`)
  and the SDK (`cd projects/start-sdk && make bundle`) before web/runtime type-checks —
  editing `shared-libs/ts-modules/start-core/lib/osBindings/*.ts` alone is not enough.
- Type-check web apps: `npm run check:ui && npm run check:setup`.
- Type-check the runtime: `cd projects/start-os/container-runtime && npm run check`.
- Build the UI: `make start-os-ui` (or `make start-os-uis` for ui + setup-wizard).
- Tests: `make test` (Rust + SDK + container-runtime), `make start-core-test`, or `make backup-fs-test` for all backup-fs library tests except the mount-based `/dev/fuse` suite.
- Format: `make start-os-format` / `make start-os-format-check` (Rust only);
  TS/web/container-runtime formatting runs through `make web-format` (root
  prettier config).
- Regenerate `start-container` man pages (committed under `man/`):
  `cargo test -p start-core export_manpage_start_container`.

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
  `shared-libs/ts-modules/start-core/lib/osBindings/index.ts` — regenerate them
  with `make start-core-ts-bindings`. `projects/start-sdk/s9pk.mk` is _not_
  generated: it is hand-maintained build plumbing that ships inside the
  published SDK, so its "DO NOT EDIT" header addresses package authors reading
  their `node_modules` copy, not this repo. Edit it here — and treat it as a
  public contract, since every package's build imports it
  ([`projects/start-sdk/AGENTS.md`](../start-sdk/AGENTS.md)).
- **Ask before destructive `make` recipes** — `update*`, `reflash`, `wormhole*`,
  image flashing, and `make clean*` consume hours/disk and may touch a live
  device.
- **The UI's `manifest.webmanifest` is rewritten before it is served** —
  `RpcContext`'s router replaces `name` and `short_name` with the server's
  hostname. Keep it out of `ngsw-config.json`: an asset group makes the service
  worker hash-check it against a body it never receives and drop out of service.
- **The `beta` feature swaps the UI seed** (`patchdb-ui-seed.beta.json`) and
  forwards to `start-core`'s `beta` feature — keep both seeds in sync when you
  change seed shape.

## Docs are part of the change

User-facing changes (UI, CLI output/flags, install/setup flow) must update the
matching page under `docs/` in the same change. Keep this AGENTS, README, and
ARCHITECTURE current when you change structure, build steps, or conventions.

## Contributor workflow

This guide covers building and contributing to the **StartOS OS product** in `projects/start-os/` — the `startbox` / `start-container` bins, the web UIs, the container runtime, and the bootable OS image. It is the source of truth for the OS-image toolchain and the build/deploy targets.

Start from the root [CONTRIBUTING.md](../../CONTRIBUTING.md) for the shared toolchain (Rust, Node 24, Docker, Make, git), branch policy, and the repo-wide commit/PR conventions; this section adds the StartOS-specific setup on top.

If you want to **package a service** for StartOS instead, see the [packaging guide](https://docs.start9.com/packaging). For other ways to help, see [start9.com/contribute](https://start9.com/contribute).

### Documentation

User-facing changes (UI, CLI, install/setup flow) must update the end-user docs under `docs/` (an mdbook served at `/start-os/`) in the same change. This product's docs: [README.md](README.md) (what it is / usage), [ARCHITECTURE.md](ARCHITECTURE.md) (how it's wired), [AGENTS.md](AGENTS.md) (agent rules and contributor workflow; `CLAUDE.md` is a one-line `@AGENTS.md` import).

### Collaboration

- [Matrix](https://matrix.to/#/#dev-startos:matrix.start9labs.com)
- Security issues: [security@start9.com](mailto:security@start9.com) — see [SECURITY.md](../../SECURITY.md)

### Prerequisites

The OS product is a thin wrapper over the shared `start-core` crate (`shared-libs/crates/start-core`), the shared TypeScript modules (`shared-libs/ts-modules`), and the SDK (`projects/start-sdk`). Build commands run from the **repo root** unless noted; the product dir is `projects/start-os`.

If you're only working on the admin UI or setup-wizard, you don't need the OS-image toolchain below — the web apps build and run standalone against mock data. See [`shared-libs/ts-modules/CONTRIBUTING.md`](../../shared-libs/ts-modules/CONTRIBUTING.md).

Beyond the shared toolchain in the [root CONTRIBUTING](../../CONTRIBUTING.md#environment-setup), **building the OS image needs multi-arch emulation and image-packaging tools** (Debian/Ubuntu):

```sh
sudo apt install -y qemu-user-static binfmt-support squashfs-tools b3sum

# Register cross-arch binfmt handlers and a buildx builder (one-time; safe to re-run)
docker run --privileged --rm tonistiigi/binfmt --install all
docker buildx create --name start9 --use 2>/dev/null || docker buildx use start9
```

#### Development Mode

For faster iteration during development:

```sh
. ./devmode.sh
```

This sets `ENVIRONMENT=dev` and `GIT_BRANCH_AS_HASH=1` to prevent rebuilds on every commit.

### Build configuration

OS builds use the repo-wide build variables (`PLATFORM`, `ENVIRONMENT`, `PROFILE`, `GIT_BRANCH_AS_HASH` — see the [root CONTRIBUTING](../../CONTRIBUTING.md#build-configuration)). The OS-specific values:

**`PLATFORM`:** `x86_64`, `x86_64-nonfree`, `aarch64`, `aarch64-nonfree`, `riscv64`, `raspberrypi`.

- `-nonfree` variants include proprietary firmware and drivers
- `raspberrypi` includes non-free components by necessity
- Platform is remembered between builds if not specified

**`ENVIRONMENT` flags:**

- `dev` — enables password SSH before setup, skips frontend compression
- `unstable` — enables assertions and debugging with a performance penalty
- `console` — enables tokio-console for async debugging

### Building

The web UIs are embedded into `startbox` at compile time (`include_dir!`), so the web build must precede the Rust build — always go through the `Makefile`, which encodes the ordering. For faster web iteration use `npm run start:ui` (see [`shared-libs/ts-modules/CONTRIBUTING.md`](../../shared-libs/ts-modules/CONTRIBUTING.md)).

```sh
cargo check -p start-os        # verify the OS bins compile (startbox, start-container)
make start-os-ui                # build the admin UI (start-os-uis for ui + setup-wizard)
make start-os                   # build all OS artifacts (bins + web + container-runtime image)
make start-os-$(IMAGE_TYPE)     # build the bootable image (start-os-iso; start-os-img on Raspberry Pi)
make start-os-deb               # Debian package (start-os-squashfs for the squashfs image)
```

`make start-core-ts-bindings` regenerates the TS bindings from the Rust types (see [Cross-layer changes](#cross-layer-changes)).

#### Deploying to a device

These targets push to a **live device** and are slow/destructive — be deliberate. For devices on the same network:

| Target                                        | Description                                     |
| --------------------------------------------- | ----------------------------------------------- |
| `start-os-update-startbox REMOTE=start9@<ip>` | Deploy binary + UI only (fastest)               |
| `start-os-update-deb REMOTE=start9@<ip>`      | Deploy full Debian package                      |
| `start-os-update REMOTE=start9@<ip>`          | OTA-style update                                |
| `start-os-emulate-reflash REMOTE=start9@<ip>` | Reflash as if using a live ISO                  |
| `start-os-update-overlay REMOTE=start9@<ip>`  | Deploy to in-memory overlay (reverts on reboot) |

To deploy a **CI build** rather than a local one — no local compile, and the image is exactly what alpha testers get:

| Target                                                         | Description                                     |
| -------------------------------------------------------------- | ----------------------------------------------- |
| `start-os-update-from-gha REMOTE=start9@<ip>`                  | Latest successful master build                  |
| `start-os-update-from-gha REMOTE=start9@<ip> RUN_ID=<id\|url>` | A specific Actions run (e.g. an integration PR) |
| `start-os-update-from-gha REMOTE=start9@<ip> BRANCH=<name>`    | That branch's latest successful build           |

It reads the platform off the device, downloads that run's `<platform>.squashfs`, and runs the same checksummed upgrade as `start-os-update-squashfs`. Needs `gh` (authenticated) and `b3sum`. Squashfs artifacts expire 14 days after the run. `scripts/update-from-gha.sh --help` documents the rest.

For devices on a different network (uses [magic-wormhole](https://github.com/magic-wormhole/magic-wormhole)):

| Target                       | Description              |
| ---------------------------- | ------------------------ |
| `start-os-wormhole`          | Send the startbox binary |
| `start-os-wormhole-deb`      | Send the Debian package  |
| `start-os-wormhole-squashfs` | Send the squashfs image  |

#### Creating a VM

Install virt-manager:

```sh
sudo apt install -y virt-manager
sudo usermod -aG libvirt $USER
sudo su $USER
virt-manager
```

Build an ISO first:

```sh
PLATFORM=$(uname -m) ENVIRONMENT=dev make start-os-iso
```

Then follow the screenshot walkthrough in [`assets/create-vm/`](assets/create-vm/) to create a new virtual machine. Key steps:

1. Create a new virtual machine
2. Browse for the ISO — create a storage pool pointing to your `results/` directory
3. Select "Generic or unknown OS"
4. Set memory and CPUs
5. Create a disk and name the VM

### Testing

```sh
make test                      # Rust + SDK + container-runtime
make start-core-test                 # backend only
make backup-fs-test                  # backup-fs library tests except the /dev/fuse suite
```

The container-runtime has its own test suite — see [container-runtime/CONTRIBUTING.md](container-runtime/CONTRIBUTING.md). Note CI builds a multi-platform matrix (apple-darwin + aarch64/x86_64/riscv64 musl); local `cargo check` is linux-only, so consider platform-specific impact.

### Formatting

```sh
make start-os-format            # format this product's Rust (core bins + backup-fs)
make start-os-format-check      # CI-style check
make web-format                 # prettier (root config) — covers web + container-runtime
```

### Cross-layer changes

When a change crosses Rust → bindings → SDK → web/runtime, verify in order:

1. `cargo check -p start-os`
2. `make start-core-ts-bindings` — regenerate ts-rs types from `start-core`
3. `cd projects/start-sdk && make bundle` — rebuild the SDK `dist` (builds `@start9labs/start-core` first and bundles it; required before the web apps / runtime can see new bindings)
4. `npm run check:ui && npm run check:setup`
5. `cd projects/start-os/container-runtime && npm run check`
