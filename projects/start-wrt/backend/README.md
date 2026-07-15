# Backend — StartWRT Rust Crates

Rust crates powering the StartWRT router daemon and CLI — members of the root monorepo Cargo workspace. Reads and writes OpenWrt UCI configuration files, manages security profiles, WiFi, Ethernet, VPN, and authentication — all exposed via JSON-RPC 2.0.

## Crates

| Crate            | Package                          | Description                                                                   |
| ---------------- | -------------------------------- | ----------------------------------------------------------------------------- |
| `ctrl`           | `startwrt-core` (lib `startwrt`) | RPC server (Axum) + CLI. Produces a single `startwrt` binary.                 |
| `uciedit`        | `uciedit`                        | Zero-copy UCI config parser/writer with atomic writes and conflict detection. |
| `uciedit_macros` | `uciedit_macros`                 | `#[derive(TypedSection)]` proc macro for compile-time-safe UCI access.        |

Other directories:

- `firstboot_config/` — Factory-default UCI configs, copied into the OpenWrt image's `/etc/config/` at image staging time (`../build/stage-files.sh`)
- `hotplug/` — Interface hotplug scripts (proxy ARP, published ports, remote access), staged into the image's `/etc/hotplug.d/iface/`
- `nftables/` — nftables include files auto-loaded by fw4, staged into the image's `/etc/nftables.d/`
- `config_experiments/` — Reference UCI configs for manual testing
- `notes/` — Research notes

## Quick Start

Run from the repo root and always scope with `-p` — a bare `cargo build`/`cargo test` targets the entire monorepo (see [AGENTS.md](AGENTS.md)):

```bash
cargo build -p startwrt-core --bin startwrt   # Build the daemon+CLI binary
cargo test -p uciedit                         # Run UCI parser tests
```

Cross-compilation for the router target (riscv64) is handled by `../build/build-rust.sh` (`projects/start-wrt/build/build-rust.sh`).

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — Backend internals: transport, modules, UCI library, error types
- [CONTRIBUTING.md](CONTRIBUTING.md) — Development guide: adding endpoints, typed sections, testing
- [AGENTS.md](AGENTS.md) — Agent/developer operating rules (`CLAUDE.md` is a one-line import of it)
- [../API_CONTRACT.md](../API_CONTRACT.md) — Complete RPC endpoint contract with Rust types
