# Architecture

This repo is the **monorepo for all Start9 products**. Each product is a thin top-level wrapper (its own bin entry points and product-specific frontend/packaging); the bulk of the code lives in shared libraries — `start-core` on the Rust side, a single Angular workspace plus the SDK on the web side.

## Tech Stack

- Backend: Rust (async/Tokio, Axum) — one library crate (`start-core`, lib name `start_core`) shared by all bins
- Frontend: Angular 22 + TypeScript + Taiga UI 5 — one workspace rooted at the repo root (`angular.json`, `package.json` at root) with shared source libraries in `shared-libs/ts-modules`
- Container runtime: Node.js/TypeScript with LXC
- Database/State: Patch-DB (in-repo at `shared-libs/crates/patch-db`) — diff-based store with reactive frontend sync
- API: JSON-RPC via rpc-toolkit (see `shared-libs/crates/start-core/rpc-toolkit.md`)
- Auth: password + session cookie, public/private key signatures, local authcookie (see `shared-libs/crates/start-core/src/middleware/auth/`)

## Repository layout

```
start-technologies/                # repo root (monorepo)
├── projects/start-os/             # OS product
│   ├── src/bin/{startbox,start-container}.rs
│   ├── web/                       #   Angular UI + setup-wizard
│   ├── container-runtime/         #   Node LXC service runtime
│   ├── debian/ apt/ assets/ build/
│   ├── *.service  services.slice
│   └── Cargo.toml                 #   → depends on start-core
├── projects/start-cli/            # start-cli bin (src/main.rs)
├── projects/start-registry/       # registrybox bin; serves the shared marketplace lib
│   └── start-registryd.service
├── projects/start-tunnel/         # tunnelbox bin + web/ (StartTunnel UI)
│   └── start-tunneld.service
├── projects/start-wrt/            # StartWRT router OS: startwrt bin + web/ (root Angular workspace) + openwrt submodule
├── projects/start-sdk/                     # @start9labs/start-sdk (source in lib/) + Makefile/s9pk.mk + docs/
├── projects/brochure-marketplace/ # marketing/landing Angular app
├── shared-libs/
│   ├── crates/
│   │   ├── start-core/            # the ENTIRE backend lib (package start-core, lib name start_core)
│   │   ├── patch-db/              #   first-party crate (Rust core + TS client)
│   │   └── …                      #   exver, imbl-value, jsonpath, pi-beep, rpc-toolkit, yasi
│   └── ts-modules/                # Angular shared libs (workspace rooted at repo root)
│       ├── shared/                #   @start9labs/shared
│       └── marketplace/           #   @start9labs/marketplace
├── angular.json  package.json     # Angular workspace root
├── Cargo.toml  Cargo.lock         # one root Cargo workspace
└── Makefile                       # top-level build/test/deploy targets
```

## Components

- **`shared-libs/crates/start-core/`** — the entire Rust backend, one library crate. Modules: `bins`, `registry`, `tunnel`, `service`, `s9pk`, `net`, `db`, `install`, `update`, `lxc`, `os_install`, `backup`, `sign`, `version`, … Handles RPC API, service lifecycle, networking (DNS, ACME, WiFi, Tor, WireGuard), backups, and database state. All product bins depend on it. See [shared-libs/crates/start-core/ARCHITECTURE.md](shared-libs/crates/start-core/ARCHITECTURE.md).

- **Product bins** — thin wrappers (feature toggles + `include_dir!` UI embed) over `start-core`:
  - `start-os` package → `startbox` (main daemon `startd`) and `start-container` (runs inside LXC containers)
  - `start-cli` → `start-cli`
  - `start-registry` → `registrybox` (package registry)
  - `start-tunnel` → `tunnelbox` (VPN/tunnel server)
  - `start-wrt` → `startwrt` (OpenWrt router OS daemon+CLI). Unlike the others this is a fuller backend of its own (UCI config, security profiles) that *reuses* `start-core`'s `net`/`util` modules (aliased as `startos`) rather than being a thin feature-toggle wrapper; its UI is an app in the root Angular workspace, and it ships as a flashable OpenWrt image, not a `.deb`/ISO.

- **`shared-libs/ts-modules/`** — shared **TypeScript** modules consumed across products. The common thread is just that they are TS — the directory is not Angular-specific. It holds the two Angular 22 / Taiga UI 5 libraries `shared` (`@start9labs/shared`) and `marketplace` (`@start9labs/marketplace`), plus the non-Angular `start-core` (`@start9labs/start-core`: the SDK's core types/ABI/effects/OS bindings, mirroring the `start-core` Rust crate; consumed directly by web and bundled into the SDK). The single Angular workspace (root `angular.json`/`package.json`) defines seven projects whose roots point into product dirs: `ui` and `setup-wizard` (`projects/start-os/web/`), `start-tunnel` (`projects/start-tunnel/web/`), `start-wrt` (`projects/start-wrt/web/`), `brochure-marketplace` (`projects/brochure-marketplace/`), plus the two Angular libraries. Apps talk to the backend exclusively via JSON-RPC. See [shared-libs/ts-modules/ARCHITECTURE.md](shared-libs/ts-modules/ARCHITECTURE.md).

- **`projects/start-os/container-runtime/`** — Node.js runtime that runs inside each service's LXC container. Loads the service's JavaScript from its S9PK and manages subcontainers; talks to the host daemon via JSON-RPC over a Unix socket. See [projects/start-os/container-runtime/AGENTS.md](projects/start-os/container-runtime/AGENTS.md).

- **`projects/start-sdk/`** — TypeScript SDK for packaging services (`@start9labs/start-sdk`), with source in `lib/`. It imports the shared `@start9labs/start-core` lib (`shared-libs/ts-modules/start-core/` — core types, ABI, effects interface, also consumed directly by web) and bundles it into its published `dist/`, so container-runtime and external service developers install a single package. Its `Makefile`/`s9pk.mk` is the source of truth for the published tarball.

- **`shared-libs/crates/patch-db/`** — first-party crate providing diff-based state sync (CBOR encoded). Backend mutations produce diffs pushed to the frontend over WebSocket for reactive UI. See the [patch-db crate](https://github.com/Start9Labs/start-technologies/tree/master/shared-libs/crates/patch-db).

## Build pipeline

One root Cargo workspace (members: the product bin crates + every shared crate under `shared-libs/crates/`, including `start-core` and the `patch-db` crates) and one Angular workspace rooted at the repo root (shared libs under `shared-libs/ts-modules`). Cross-layer changes flow in one direction:

```
Rust (shared-libs/crates/start-core)
  → make ts-bindings: ts-rs export → shared-libs/crates/start-core/bindings/ → rsync to shared-libs/ts-modules/start-core/lib/osBindings/
    → start-core build (cd shared-libs/ts-modules/start-core && make dist) → dist/
      → shared-libs/ts-modules + web apps consume it (via @start9labs/start-core)
    → SDK build (cd projects/start-sdk && make bundle) → dist/ (bundles @start9labs/start-core)
      → projects/start-os/container-runtime consumes the SDK dist/ (via @start9labs/start-sdk) + @start9labs/start-core
```

Key make targets along the chain:

| Step | Command | What it does |
|---|---|---|
| 1 | `cargo check -p start-core` | Verify the backend lib compiles |
| 2 | `make ts-bindings` | Export ts-rs types → rsync to `shared-libs/ts-modules/start-core/lib/osBindings/` |
| 3 | `cd projects/start-sdk && make bundle` | Build the SDK `dist/` (builds `@start9labs/start-core` first and bundles it) |
| 4 | `npm run check` | Type-check Angular projects (from the repo root) |
| 5 | `cd projects/start-os/container-runtime && npm run check` | Type-check the runtime |

**Important**: editing `shared-libs/ts-modules/start-core/lib/osBindings/*.ts` alone is NOT enough — rebuild start-core (and the SDK bundle, step 3) before web/container-runtime can see the change.

## Cross-layer verification

When a change spans Rust, SDK, web, and container-runtime, verify in the order above (1→5). `make ts-bindings` runs the `start-core` export and rsyncs `shared-libs/crates/start-core/bindings/` → `shared-libs/ts-modules/start-core/lib/osBindings/`; the built `@start9labs/start-core` is what web references, and the SDK bundle (step 3) is what container-runtime references, not the source files.

## Data flow: backend → frontend

StartOS uses Patch-DB for reactive state sync:

1. The backend mutates state via `db.mutate()`, producing CBOR diffs.
2. Diffs are pushed to the frontend over a persistent WebSocket.
3. The frontend applies diffs to its local state copy and notifies observers.
4. Components watch specific DB paths via `PatchDB.watch$()`, receiving updates reactively.

The UI is therefore eventually consistent with the backend — after a mutating API call, the frontend waits for the corresponding PatchDB diff before resolving, so the UI reflects the result immediately.

## Further reading

- [shared-libs/crates/start-core/ARCHITECTURE.md](shared-libs/crates/start-core/ARCHITECTURE.md) — Rust backend
- [shared-libs/ts-modules/ARCHITECTURE.md](shared-libs/ts-modules/ARCHITECTURE.md) — Angular frontend
- [projects/start-os/container-runtime/AGENTS.md](projects/start-os/container-runtime/AGENTS.md) — container runtime
- [shared-libs/crates/start-core/rpc-toolkit.md](shared-libs/crates/start-core/rpc-toolkit.md) — JSON-RPC handler patterns
- [shared-libs/crates/start-core/s9pk-structure.md](shared-libs/crates/start-core/s9pk-structure.md) — S9PK package format
- [shared-libs/crates/start-core/exver.md](shared-libs/crates/start-core/exver.md) — extended versioning format
- [shared-libs/crates/start-core/VERSION_BUMP.md](shared-libs/crates/start-core/VERSION_BUMP.md) — version bumping guide
