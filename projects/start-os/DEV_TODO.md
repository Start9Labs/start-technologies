# AI Agent TODOs

Pending tasks for AI agents. Remove items when completed.

## Features

- [ ] Extract TS-exported types into a lightweight sub-crate for fast binding generation

  **Problem**: `make start-core-ts-bindings` compiles the entire `start-os` crate (with all dependencies: tokio,
  axum, openssl, etc.) just to run test functions that serialize type definitions to `.ts` files.
  Even in debug mode, this takes minutes. The generated output is pure type info — no runtime code
  is needed.

  **Goal**: Generate TS bindings in seconds by isolating exported types in a small crate with minimal
  dependencies.

  **Approach**: Create a `core/bindings-types/` sub-crate containing (or re-exporting) all 168
  `#[ts(export)]` types. This crate depends only on `serde`, `ts-rs`, `exver`, and other type-only
  crates — not on tokio, axum, openssl, etc. Then `build-ts.sh` runs `cargo test -p bindings-types`
  instead of `cargo test -p start-os`.

  **Challenge**: The exported types are scattered across `core/src/` and reference each other and
  other crate types. Extracting them requires either moving the type definitions into the sub-crate
  (and importing them back into `start-os`) or restructuring to share a common types crate.

- [ ] Auto-configure port forwards via UPnP/NAT-PMP/PCP - @dr-bonez

  **Goal**: When a binding is marked public, automatically configure port forwards on the user's router
  using UPnP, NAT-PMP, or PCP, instead of requiring manual router configuration. Fall back to
  displaying manual instructions (the port forward mapping from patch-db) when auto-configuration is
  unavailable or fails.

- [ ] Use TLS-ALPN challenges for check-port when addSsl - @dr-bonez

  **Problem**: The `check_port` RPC in `core/src/net/gateway.rs` currently uses an external HTTP
  service (`ifconfig_url`) to verify port reachability. This doesn't check whether the port is forwarded to the right place, just that it's open. there's nothing we can do about this if it's a raw forward, but if it goes through the ssl proxy we can do a better verification.

  **Goal**: When a binding has `addSsl` enabled, use TLS-ALPN-01 challenges to verify port
  reachability instead of (or in addition to) the plain TCP check. This more accurately validates
  that the SSL port is properly configured and reachable.
