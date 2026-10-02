# AGENTS.md — shared-libs

`shared-libs/` groups cross-product Rust crates and TypeScript modules. It has no
standalone build. Read the enclosing root guidance and the `AGENTS.md` for the
library being changed. `CLAUDE.md` imports this file.

## Layout

- `crates/start-core/` — the backend used by the product binaries.
- `crates/rpc-toolkit/` — JSON-RPC handlers, transports, CLI bindings and optional
  TypeScript method-tree generation.
- `crates/visit-rs/` and `crates/visit-rs-derive/` — value/type reflection and
  directional serde JSON-shape traversal. Both start-core and StartWRT use the
  in-workspace runtime crate; rpc-toolkit uses it for its opt-in `ts` feature.
- The other first-party Rust crates have their own scoped guidance.
- `ts-modules/` — Angular `shared`/`marketplace` libraries and the non-Angular
  `start-core` TypeScript library. The Angular workspace is at the repo root.

## Contributor workflow

Build and test Rust by package name from the root workspace. `make start-core-test`
exercises the backend; `make rpc-toolkit-test` exercises reflection and RPC bindings
in their supported feature configurations, including generated-module checks with
the root pinned TypeScript compiler. The latter installs its Node prerequisites
through the existing workspace make rules.

`make start-core-format` and `make start-core-format-check` include every shared
Rust crate. Native Rust builds are Linux-only; CI also verifies the product target
matrix. Platform-specific dependencies and APIs must preserve those targets.

For shared TypeScript changes, install root dependencies with `npm ci`, run
`npm run build:deps` before `npm run check`, and follow `ts-modules/AGENTS.md`.
Cross-layer rebuild ordering lives in the root `ARCHITECTURE.md`. Frontend code
also follows the `start9-frontend` skill.
