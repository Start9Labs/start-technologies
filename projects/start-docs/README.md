# Start9 Docs Site

The documentation site for [Start9](https://start9.com) products — StartOS, StartTunnel, Service Packaging, and Bitcoin Guides.

**Live site:** [docs.start9.com](https://docs.start9.com)

This project (the `projects/start-docs/` directory of the `start-os` monorepo) owns the **site build infrastructure**, the **landing page**, and the **Bitcoin Guides** book. The per-product mdBooks now live in their own product directories; this project's build wires them together into one deployed site.

## Books and where they live

| Book              | URL                                                                       | Source dir              | Content                                                      |
| ----------------- | ------------------------------------------------------------------------- | ----------------------- | ------------------------------------------------------------ |
| StartOS           | [docs.start9.com/start-os](https://docs.start9.com/start-os/)             | `../start-os/docs/`     | Setup, services, networking, backups, system admin, firmware |
| StartTunnel       | [docs.start9.com/start-tunnel](https://docs.start9.com/start-tunnel/)     | `../start-tunnel/docs/` | Installation, subnets, devices, port forwarding              |
| Service Packaging | [docs.start9.com/packaging](https://docs.start9.com/packaging/)           | `../start-sdk/docs/`    | Developer guide for building and publishing StartOS services |
| Bitcoin Guides    | [docs.start9.com/bitcoin-guides](https://docs.start9.com/bitcoin-guides/) | `bitcoin-guides/`       | Running Bitcoin and related services on StartOS              |

The first three books were moved out of this directory into their product dirs so each book sits next to the code it documents. `build.sh` maps each book name to its source dir, so build output and deployed URLs are unchanged. Only `bitcoin-guides` still lives here.

## What's in this project

```
projects/start-docs/
├── build.sh           ← builds every book into the docs/ output dir
├── serve.sh           ← build + local dev server
├── versions.conf      ← book → version list (single source of truth)
├── theme/             ← shared mdBook theme (CSS/JS/favicon), symlinked by every book
├── landing/           ← static landing page served at docs.start9.com/
├── bitcoin-guides/    ← the Bitcoin Guides mdBook
└── scripts/           ← build-time tooling (llms.txt generator)
```

## Product context

- **StartOS 0.4.0** — networking strategies not built into StartOS core (Tor, P2P tunnels, etc.) are distributed as marketplace services. Outbound gateways support a system default plus per-service override.
- **Service Packaging** — single source of truth for service packaging; code examples are based on the [hello-world](https://github.com/Start9Labs/hello-world-startos) template.
- **StartTunnel** — a WireGuard-based gateway service for clearnet access.
- **Bitcoin Guides** — wallet indexes, Electrum servers, archival vs pruned nodes, LND migration. The Bitcoin package integrates btc-rpc-proxy for on-demand block fetching, so pruned nodes work transparently with multiple downstream services.

## How it works

Built with [mdBook](https://rust-lang.github.io/mdBook/). Each book is an independent mdBook instance sharing the common `theme/`. On push to `master` (paths under `projects/start-docs/`, `projects/start-os/docs/`, `projects/start-tunnel/docs/`, or `projects/start-sdk/docs/`), the `docs-deploy.yml` workflow:

1. Builds all books into versioned `docs/<book>/<version>/` directories
2. Generates `llms.txt` and `llms-full.txt` for LLM consumption
3. rsyncs to the VPS and regenerates nginx routing from `versions.conf`

## Documentation

- [CONTRIBUTING.md](CONTRIBUTING.md) — local setup and how to submit changes
- [ARCHITECTURE.md](ARCHITECTURE.md) — how the build is structured
- [AGENTS.md](AGENTS.md) — operating rules for AI developers (`CLAUDE.md` is a one-line `@AGENTS.md` import)
