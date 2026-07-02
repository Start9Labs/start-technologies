<div align="center">
  <img src="shared-libs/ts-modules/shared/assets/img/icon.png" alt="StartOS Logo" width="16%" />
  <h1 style="margin-top: 0;">Start Technologies</h1>
  <a href="https://github.com/Start9Labs/start-technologies/releases">
    <img alt="GitHub release (with filter)" src="https://img.shields.io/github/v/release/start9labs/start-technologies?logo=github">
  </a>
  <a href="https://github.com/Start9Labs/start-technologies/actions/workflows/startos-iso.yaml">
    <img src="https://github.com/Start9Labs/start-technologies/actions/workflows/startos-iso.yaml/badge.svg">
  </a>
  <a href="https://docs.start9.com">
    <img alt="Static Badge" src="https://img.shields.io/badge/docs-orange?label=%F0%9F%91%A4%20support">
  </a>
  <a href="https://matrix.to/#/#dev-startos:matrix.start9labs.com">
    <img alt="Static Badge" src="https://img.shields.io/badge/developer-matrix-darkcyan?logo=matrix">
  </a>
  <a href="https://start9.com">
    <img alt="Website" src="https://img.shields.io/website?up_message=online&down_message=offline&url=https%3A%2F%2Fstart9.com&logo=website&label=%F0%9F%8C%90%20website">
  </a>
</div>

## What is this?

This repository is the **monorepo for all Start9 products**. Its flagship is **StartOS** — an open-source Linux distribution for running a personal server, handling discovery, installation, network configuration, data backup, dependency management, and health monitoring of self-hosted services.

All products share a single Rust backend library (`start-core`) and a single Angular workspace; each product is a thin wrapper under `projects/` that adds only its own entry point and any product-specific frontend or packaging, while shared code lives at the top level under `shared-libs/`.

| Directory                        | Product          | What it is                                                                                                |
| -------------------------------- | ---------------- | --------------------------------------------------------------------------------------------------------- |
| `projects/start-os/`             | StartOS          | The server OS — `startbox`/`start-container` bins, web UI + setup wizard, container runtime, OS packaging |
| `projects/start-cli/`            | start-cli        | CLI for managing servers, registries, and packaging                                                       |
| `projects/start-registry/`       | start-registry   | Package registry server (`registrybox`); serves the marketplace UI                                        |
| `projects/start-tunnel/`         | StartTunnel      | VPN/forwarding server (`tunnelbox`) + its web UI                                                          |
| `projects/start-wrt/`            | StartWRT         | OpenWrt-based router OS (`startwrt` bin + embedded web UI), flashable image for the SpaceMiT K1           |
| `projects/start-sdk/`            | Start SDK        | `@start9labs/start-sdk` for building StartOS service packages                                             |
| `projects/brochure-marketplace/` | Marketplace site | Public marketplace/landing site (marketplace.start9.com)                                                  |
| `projects/start-docs/`           | Docs site        | The documentation website (docs.start9.com)                                                               |
| `shared-libs/crates/start-core/` | —                | The entire Rust backend library shared by all bins                                                        |
| `shared-libs/ts-modules/`        | —                | Shared Angular libraries (the Angular workspace is rooted at the repo root)                               |
| `shared-libs/crates/patch-db/`   | —                | Diff-based reactive state store (first-party crate)                                                       |

**Tech stack:** Rust backend (Tokio/Axum), Angular frontend (Taiga UI), Node.js container runtime with LXC, and a custom diff-based database ([Patch-DB](https://github.com/Start9Labs/start-technologies/tree/master/shared-libs/crates/patch-db)) for reactive state synchronization. Services run in isolated LXC containers, packaged as S9PKs — a signed, merkle-archived format supporting partial downloads and cryptographic verification.

See [ARCHITECTURE.md](ARCHITECTURE.md) for how the pieces fit together.

## What can you do with StartOS?

StartOS lets you self-host services that would otherwise depend on third-party cloud providers — giving you full ownership of your data and infrastructure. Browse available services on the [Start9 Marketplace](https://marketplace.start9.com/), including:

- **Bitcoin & Lightning** — full Bitcoin node, Lightning node, BTCPay Server, and other payment infrastructure
- **Communication** — Matrix, SimpleX, and other messaging platforms
- **Cloud Storage** — Nextcloud, Vaultwarden, and other productivity tools

Services are added by the community. If a service you want isn't available, you can [package it yourself](https://docs.start9.com/packaging/).

## Getting StartOS

### Buy a Start9 server

The easiest path. [Buy a server](https://store.start9.com) from Start9 and plug it in.

### Build your own

Follow the [install guide](https://docs.start9.com/start-os/installing-startos.html) to install StartOS on your own hardware.

### Build from source

```sh
git clone https://github.com/Start9Labs/start-technologies.git
cd start-technologies
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the shared toolchain and development workflow, and [projects/start-os/CONTRIBUTING.md](projects/start-os/CONTRIBUTING.md) to build a StartOS image.

## The rest of the monorepo

StartOS is the flagship, but it shares this repo with the rest of the Start9 stack:

- **[StartTunnel](projects/start-tunnel/)** — a self-hosted VPN / reverse-proxy server that gives a StartOS box a public address and clearnet port forwarding without relying on a third-party tunnel.
- **[StartWRT](projects/start-wrt/)** — an OpenWrt-based router OS for home self-hosting: per-profile subnets and WiFi, inbound/outbound VPN, DDNS, and secure remote access, shipped as a flashable image for the SpaceMiT K1.
- **[start-cli](projects/start-cli/)** — the command-line client for StartOS servers and registries, and the tool that builds and signs service packages (`.s9pk`).
- **[Start SDK](projects/start-sdk/)** — the `@start9labs/start-sdk` TypeScript SDK and packaging toolchain for wrapping any app into an installable StartOS service.
- **[start-registry](projects/start-registry/)** — the registry server that hosts and serves marketplaces of packaged services.
- **[Marketplace site](projects/brochure-marketplace/)** — the public marketplace at [marketplace.start9.com](https://marketplace.start9.com), built on the same UI components the OS uses.
- **[Docs site](projects/start-docs/)** — the documentation website at [docs.start9.com](https://docs.start9.com).

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md) — how the monorepo fits together
- [CONTRIBUTING.md](CONTRIBUTING.md) — environment setup, build, test, and format workflow
- [AGENTS.md](AGENTS.md) — AI-developer/agent operating rules (`CLAUDE.md` is a one-line `@AGENTS.md` import)

## Contributing

There are multiple ways to contribute: work directly on a product in this repo, package a service for the marketplace, or help with documentation and guides. See [CONTRIBUTING.md](CONTRIBUTING.md) or visit [start9.com/contribute](https://start9.com/contribute/).

To report security issues, email [security@start9.com](mailto:security@start9.com).
