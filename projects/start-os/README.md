# StartOS

StartOS is an open-source Linux distribution for running a personal server. It
handles discovery, installation, network configuration, data backup, dependency
management, and health monitoring of self-hosted services. Services run in
isolated LXC containers, packaged as signed, merkle-archived `.s9pk` files.

This directory is the **StartOS OS product** within the monorepo. It is a thin
wrapper that ships the OS:

- the `startbox` daemon binary (`startd` / `start-cli` multi-call applet) and the
  `start-container` binary that runs inside each service LXC,
- the web UIs (`ui` = admin dashboard, `setup-wizard` = first-boot setup),
- the `container-runtime` (Node.js service runtime that runs inside package LXCs),
- the systemd units and OS image packaging glue.

The bulk of the backend logic lives in the shared `start-core` crate
(`../../shared-libs/crates/start-core`), and the Angular apps here consume the shared web
libraries under `../../shared-libs/ts-modules` and the SDK base from `../start-sdk`.

## Tech stack

- **Backend:** Rust (Tokio async, Axum), built on the `start-core` crate.
- **Frontend:** Angular 22 + Taiga UI 5 (apps live under `web/`, shared libs
  under `../../shared-libs/ts-modules`).
- **Container runtime:** Node.js/TypeScript managing LXC service containers.
- **State/sync:** Patch-DB (`../../shared-libs/crates/patch-db`) — diff-based store that pushes
  CBOR diffs to the frontend over WebSocket for reactive, poll-free UI updates.

## Layout

```
start-os/
├── src/bin/startbox.rs         # startd + start-cli multi-call applet; embeds the UIs
├── src/bin/start-container.rs  # runs inside each service LXC
├── Cargo.toml                  # package "start-os" → depends on start-core
├── web/
│   ├── ui/                     # admin dashboard (Angular app "ui")
│   ├── setup-wizard/           # first-boot setup (Angular app "setup-wizard")
│   └── patchdb-ui-seed*.json   # seed state embedded into startbox
├── container-runtime/          # Node.js LXC service runtime (own README/AGENTS)
├── docs/                       # mdbook (book title "StartOS"); served at /start-os/
├── startd.service              # systemd units + cgroup slice for service containers
├── services.slice
├── startos-shutdown.service
└── startos-restart.service
```

OS-image packaging shared across products lives at the repo root: `debian/`
(maintainer scripts), `apt/`, and `build/` (image-recipe, firmware, env scripts);
this product's own assets live in `assets/` here. The root `Makefile` drives the
full ISO/img build.

## Quickstart

Build commands run from the **repo root** (one Cargo workspace, one Angular
workspace). See [CONTRIBUTING.md](CONTRIBUTING.md) for environment setup
(including the OS-image toolchain) and the full build/test workflow.

```sh
# from the repo root
cargo check -p start-os                 # verify the OS bins compile
make start-os-ui                          # build the admin UI
make start-os                             # build all OS artifacts (bins + web + runtime image)
make start-os-$(IMAGE_TYPE)               # build the bootable image (iso, or img on Pi)
```

`make` targets relevant to this product: `start-os-ui`, `start-os-uis`, `start-os`, `start-os-deb`,
`start-os-install`, `start-os-$(IMAGE_TYPE)`, `start-os-squashfs`,
`start-os-update`/`start-os-emulate-reflash`/`start-os-wormhole*` (deploy to a live
device — slow and destructive). Run `make test` for the full Rust + SDK + runtime
test suite.

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — how the OS product is structured and how
  data flows backend → frontend.
- [CONTRIBUTING.md](CONTRIBUTING.md) — build/test/format workflow for this product.
- [AGENTS.md](AGENTS.md) — operating rules for AI developers working here.
- [CHANGELOG.md](CHANGELOG.md) — release history.
- End-user docs: [docs.start9.com](https://docs.start9.com) (built from `docs/`).

## Getting StartOS as a user

- **Buy a server** from [store.start9.com](https://store.start9.com) — easiest path.
- **Install on your own hardware** — follow the
  [install guide](https://docs.start9.com/start-os/installing-startos.html).
- Browse services on the [Start9 Marketplace](https://marketplace.start9.com/).

To report security issues, email [security@start9.com](mailto:security@start9.com).
