# AI Agent TODOs

Pending tasks for AI agents. Remove items when completed.

## Features

- [ ] Extract TS-exported types into a lightweight sub-crate for fast binding generation

  **Problem**: `make start-core-ts-bindings` compiles `start-core` with its backend dependencies
  to reflect the actual server handlers and standalone DTO exports. Even in debug mode,
  a rebuild can take minutes.

  **Goal**: Generate TS bindings in seconds by isolating exported types in a small crate with minimal
  dependencies.

  **Constraint**: `visit-rs` owns the renderer, and standalone exports belong to annotations.
  The RPC trees come from production contexts; a separate hand-maintained handler or export list
  would lose their freshness guarantee. Any type extraction must preserve that ownership and
  account for the DTOs scattered across `shared-libs/crates/start-core/src/`.

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
