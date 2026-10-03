# Shared Libraries

Cross-product shared code for the Start9 monorepo. Everything here is consumed by
the thin product crates/apps at `projects/` (`projects/start-os/`,
`projects/start-cli/`, `projects/start-registry/`, `projects/start-tunnel/`,
`projects/start-wrt/`,
`projects/start-sdk/`, `projects/brochure-marketplace/`); little ships from
here on its own.

## Layout

- **`crates/start-core/`** — the entire Rust backend, packaged as the `start-core`
  crate (lib name `start_core`). Every product binary (`startbox`/`start-container`,
  `start-cli`, `registrybox`, `tunnelbox`, `startwrt`) depends on it. See
  [`crates/start-core/README.md`](crates/start-core/README.md).

- **`crates/visit-rs/`** and **`crates/visit-rs-derive/`** — value/type reflection
  and serde JSON-shape traversal, shared by the backend and RPC bindings.

- **`ts-modules/`** — shared TypeScript modules; two Angular source libraries in the single Angular workspace rooted at the repo
  root (`angular.json`, `package.json`, `package-lock.json`):
  - `ts-modules/shared/` → `@start9labs/shared` (API clients, components, i18n, styles)
  - `ts-modules/marketplace/` → `@start9labs/marketplace` (service discovery UI)
  - `ts-modules/start-core/` → `@start9labs/start-core` (non-Angular types/ABI/effects/OS bindings, built separately and bundled into the SDK)

  The product apps (`projects/start-os/web/ui`, `projects/start-os/web/setup-wizard`,
  `projects/start-tunnel/web`, `projects/start-wrt/web`, `projects/brochure-marketplace/src`) all live in their
  product dirs but build through this workspace and import these libraries. See
  [`ts-modules/README.md`](ts-modules/README.md).

## Quickstart

Rust backend:

```bash
# from the repo root
cargo build -p start-core            # build the shared lib
make start-core-test                 # scoped backend tests
```

Web:

```bash
# from the repo root
npm ci
npm run build:deps                   # build @start9labs/start-core + patch-db client
npm run check                        # typecheck all projects
```

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — how the two sub-libs fit into the monorepo
- [AGENTS.md](AGENTS.md) — build/test/format workflow
