# Backend — StartWRT Rust Crates

Rust workspace powering the StartWRT router daemon and CLI. Reads and writes OpenWrt UCI configuration files, manages security profiles, WiFi, Ethernet, VPN, and authentication — all exposed via JSON-RPC 2.0.

## Crates

| Crate | Package | Description |
|-------|---------|-------------|
| `ctrl` | `startwrt-ctrl` | RPC server (Axum) + CLI. Produces a single `startwrt` binary. |
| `uciedit` | `uciedit` | Zero-copy UCI config parser/writer with atomic writes and conflict detection. |
| `uciedit_macros` | `uciedit_macros` | `#[derive(TypedSection)]` proc macro for compile-time-safe UCI access. |

Other directories:
- `firstboot_config/` — Factory-default UCI configs embedded in the binary via `include_dir`
- `config_experiments/` — Reference UCI configs for manual testing

## Quick Start

```bash
cargo build                          # Build all crates
cargo build -p startwrt-ctrl         # Build ctrl only
cargo test -p uciedit                # Run UCI parser tests
```

Cross-compilation for the router target (riscv64) is handled by `build/build-rust.sh`.

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — Backend internals: transport, modules, UCI library, error types
- [CONTRIBUTING.md](CONTRIBUTING.md) — Development guide: adding endpoints, typed sections, testing
- [CLAUDE.md](CLAUDE.md) — AI assistant quick reference
- [../API_CONTRACT.md](../API_CONTRACT.md) — Complete RPC endpoint contract with Rust types
