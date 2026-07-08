# Architecture

StartOS is a Linux distribution purpose-built for running a personal server. Unlike general-purpose operating systems designed for desktops and laptops, StartOS provides a graphical interface for discovering, installing, configuring, and managing self-hosted services — no command line required.

This page describes how StartOS is designed and how its components fit together. For implementation details, see the [start-os repository](https://github.com/Start9Labs/start-technologies).

## Components

StartOS is composed of four major components:

- **Core** — A Rust backend that manages system state, service lifecycles, networking, storage, and the API. It compiles into a single binary (`startbox`) that is symlinked to serve as the system daemon (`startd`), the CLI (`start-cli`), and other utilities.
- **Web UI** — An Angular frontend that provides the admin interface, setup wizard, and marketplace. It communicates with the backend over JSON-RPC and WebSocket.
- **Container Runtime** — A Node.js process that runs inside each service's LXC container, loading the package's JavaScript and managing subcontainers, health checks, and effects callbacks.
- **SDK** — A TypeScript library that package developers use to define service metadata, daemons, actions, interfaces, and other behaviors. The SDK compiles into the JavaScript bundle shipped inside each package.

## Service Model

Every service on StartOS runs inside its own isolated [LXC container](https://linuxcontainers.org/). This provides process isolation, filesystem isolation, and network namespacing without the overhead of full virtual machines.

### S9PK Package Format

Services are distributed as `.s9pk` files — a custom archive format built on a merkle tree. An S9PK contains:

- **manifest.json** — Package metadata: ID, title, version, license, descriptions, dependency declarations, and image definitions.
- **icon** — The package icon displayed in the marketplace and UI.
- **LICENSE.md** — The applicable license.
- **javascript.squashfs** — The compiled SDK code that defines the service's behavior on StartOS (daemons, actions, health checks, interfaces, etc.).
- **images/** — Container root filesystem images (squashfs), organized by CPU architecture (x86_64, aarch64, riscv64).
- **assets.squashfs** — Optional static assets mounted read-only into the container.

The merkle archive format enables partial downloads, integrity verification of subsets, efficient updates (only changed portions need re-downloading), and size limit enforcement before completing a download.

S9PK files are cryptographically signed (Ed25519) so that users and registries can verify package authenticity.

### Service Lifecycle

A service moves through several lifecycle stages, each of which can trigger package-defined code:

1. **Install** — The S9PK is unpacked, container images are extracted, volumes are created, and the package's `setupOnInit` function runs with `kind: 'install'`. This is where packages generate initial secrets, create default configuration, and prompt the user for required setup via tasks.
2. **Actions** — Actions are operations defined by the package that appear as buttons in the UI. They can display information (e.g., show admin credentials), accept user input (e.g., configure SMTP), or modify the service's state. Actions can run whether the service is started, stopped, or both.
3. **Tasks** — Tasks are notifications that prompt the user to run a specific action. Packages create tasks during initialization or at runtime to guide the user through required setup steps. Tasks have a severity level: _critical_ tasks block the service from starting until completed, while lower-severity tasks are informational.
4. **Start** — The package's `setupMain` function runs, which defines daemons (long-running processes), oneshots (startup tasks like migrations), and health checks. Daemons run inside subcontainers created from the package's images.
5. **Update** — When a new version is installed, the package's version migration code runs, transforming stored data as needed. The `setupOnInit` function runs again with `kind: 'install'`.
6. **Backup** — StartOS creates an encrypted backup of the service's designated volumes. Services can exclude data that is recoverable by other means (e.g., Bitcoin excludes the blockchain).
7. **Restore** — A backup is decrypted and restored. The `setupOnInit` function runs with `kind: 'restore'`, allowing the package to re-register triggers or re-prompt the user.
8. **Uninstall** — The container and its volumes are removed.

### Container Architecture

When a service starts, the container runtime loads the package's JavaScript and uses it to create subcontainers from the package's images. The relationship looks like this:

```
LXC Container (managed by StartOS)
└── Container Runtime (Node.js)
    └── Package JavaScript (from s9pk)
        ├── SubContainer A (e.g., the main application)
        ├── SubContainer B (e.g., a database)
        └── Health Checks
```

The container runtime communicates with the StartOS host via JSON-RPC over a Unix socket. This is how packages invoke "effects" — host-level operations like reading configuration, resolving hostnames, accessing the network, or creating user-facing tasks.

### Volumes

Each service has one or more named volumes for persistent data. Volumes survive container restarts, updates, and restores. They are id-mapped to the container's user namespace for security. Packages declare which volumes to include in backups.

## Networking

StartOS provides multiple ways to access services, all managed through the UI.

### LAN Access

Services are accessible over the local network using mDNS hostnames (`your-server-name.local`). StartOS runs its own DNS resolver and issues TLS certificates from a local Certificate Authority (CA). Users trust this CA on their client devices to get HTTPS connections over LAN without browser warnings.

### Tor

Tor is available as a marketplace service. Once installed, users can enable onion addresses for any service through the Tor service's actions. Tor provides access from anywhere without port forwarding or exposing your IP address.

### Clearnet

Services can be exposed to the public internet using a domain name. This requires a clearnet gateway — a virtual private router running on a VPS with a public IP. [StartTunnel](/start-tunnel/) is a purpose-built VPR for this, but any WireGuard configuration can be used for outbound traffic.

### Gateways

A gateway is a WireGuard tunnel that routes traffic between a StartOS server and the outside world. Gateways can be:

- **Inbound + Outbound** — Routes both incoming and outgoing traffic (e.g., StartTunnel). This enables clearnet access to services.
- **Outbound only** — Routes only outgoing traffic through the tunnel (e.g., a commercial VPN). This masks your server's IP for outbound connections.

StartOS auto-detects the gateway type from the WireGuard configuration file. A system-wide default gateway can be set, and individual services can override it.

## State Management

StartOS uses **Patch-DB**, a custom diff-based database, to manage system state. The key property of Patch-DB is reactive synchronization: when the backend mutates state, the frontend receives only the diff (patch) over a WebSocket connection. This means the UI always reflects the current system state without polling.

The database has two layers:

- **Public model** — Synced to the frontend. Contains everything the UI needs: service status, installed packages, system settings, network configuration, notifications.
- **Private model** — Backend-only. Contains internal state like cryptographic keys, session tokens, and operational data that should never leave the server.

## Security

### Container Isolation

Each service runs in its own LXC container with:

- Separate filesystem (id-mapped volumes)
- Network namespace isolation
- Resource limits
- No direct access to host resources except through the effects API

### Encrypted Backups

All backups are encrypted using the user's master password. Backups can be stored on physical drives or network shares (SMB/CIFS). Each service's backup retains the password that was active when the backup was created.

### Authentication

The web UI is protected by password authentication with session cookies. The API uses JSON-RPC with session-based auth. SSH access is available for advanced users but is not required for normal operation.

### Package Signing

S9PK files are signed with Ed25519 keys. The registry and StartOS verify signatures before installing packages, ensuring that packages have not been tampered with.

## Tech Stack

| Component         | Technology                            |
| ----------------- | ------------------------------------- |
| Backend           | Rust (async Tokio, Axum)              |
| Frontend          | Angular, TypeScript, Taiga UI         |
| Container Runtime | Node.js, TypeScript                   |
| Containers        | LXC                                   |
| Database          | Patch-DB (custom, diff-based)         |
| API               | JSON-RPC                              |
| Package Format    | S9PK (merkle archive, Ed25519 signed) |
| Networking        | WireGuard, Tor (Arti), mDNS, ACME     |
| Supported Archs   | x86_64, aarch64, riscv64              |

## Source Code

StartOS is fully open source. The main repository is [Start9Labs/start-technologies](https://github.com/Start9Labs/start-technologies) on GitHub. See the repository's CONTRIBUTING.md for build instructions and development setup.
