# Packaging Guide

StartOS is a _Server OS_ -- a Linux distribution optimized for administering servers. While operating systems like Mac, Windows, and Ubuntu are designed for client devices such as phones and laptops, StartOS provides a graphical interface for server administration that eliminates the need to "pop the hood" and use the command line.

Through the StartOS web interface, users can discover, download, install, configure, monitor, back up, and generally manage any variety of self-hosted, open-source software.

## Designed for AI-Assisted Development

StartOS service packaging is designed to be done with an AI coding agent. This guide, the SDK, and every existing package are structured so that an AI assistant can read the docs, study real packages, and write or modify package code with minimal human intervention. You do not need to be an expert TypeScript developer -- you need to understand _what_ your service requires and let the AI handle _how_ to implement it.

The recommended setup is [Claude Code](https://claude.com/claude-code) with this guide and your package in the same workspace, scaffolded by `start-cli s9pk init-workspace`. See [Environment Setup](./environment-setup.md#set-up-your-packaging-workspace) and the [Quick Start](./quick-start.md) to get going.

## What is a StartOS Package?

What makes this experience possible is a unique package format (`.s9pk`) that permits services to take advantage of StartOS APIs. In its most basic form, a package is a thin metadata wrapper around a service that allows it to be discovered, installed, and run on StartOS. Beyond that, the StartOS APIs grant developers an incredible degree of creative capacity to define the end-user experience for their service. Developers can:

- Display instructions and tooltips
- Present warnings under certain conditions
- Run arbitrary code on install, update, and uninstall
- Represent configuration files as validated forms with all varieties of form inputs
- Define scripts and commands that present as buttons with optional inputs
- Write health checks that run on an interval and are optionally displayed
- Automatically install and configure dependencies
- Maintain state and optionally expose particular values to users or dependent services
- Grant users flexible networking options such as LAN, Tor, and clearnet
- Offer one-click, encrypted backups of targeted data

## Where to Start

1. **Set up your environment** — Follow [Environment Setup](./environment-setup.md), including the Claude Code section.
2. **Build your first package** — Follow [Quick Start](./quick-start.md) to create, build, and install the Hello World template.
3. **Use recipes to build your service** — Browse [Recipes](./recipes.md) to find the patterns you need. Each recipe describes _what_ to do, links to reference pages for API details, and points to real packages for working code. Your AI agent reads these docs and writes the code.

## Getting Started

- [Environment Setup](./environment-setup.md) - Install the required development tools
- [Quick Start](./quick-start.md) - Create, build, and install your first package
- [New Package Checklist](./new-package-checklist.md) - Turn the scaffolded template into your service
- [Development Workflow](./workflow.md) - How to behave while working on a package
- [Maintaining a Package](./maintaining-a-package.md) - Branches, upstream updates, and sibling dependencies over a package's life
- [Agent Context](./agent-context.md) - The `AGENTS.md` linked into every packaging workspace

## Recipes

Intent-driven guides for common packaging objectives. These are the primary entry point for both you and your AI coding agent.

- [Browse All Recipes](./recipes.md) - Browse all recipes by intent

### Configuration

- [Set Up a Basic Service](./recipe-basic-service.md)
- [Package a Prebuilt Docker Image](./recipe-prebuilt-image.md)
- [Create Configuration Actions](./recipe-config-actions.md)
- [Generate Config Files](./recipe-config-files.md)
- [Pass Config via Environment Variables](./recipe-env-vars.md)
- [Hardcode Config Values](./recipe-hardcode-config.md)
- [Set a Primary URL](./recipe-primary-url.md)
- [Set Up SMTP / Email](./recipe-smtp.md)

### Credentials & Access Control

- [Auto-Generate Internal Secrets](./recipe-internal-secrets.md)
- [Prompt User to Create Admin Credentials](./recipe-admin-credentials.md)
- [Reset a Password](./recipe-reset-password.md)
- [Gate User Registration](./recipe-registration-gating.md)

### Setup & Lifecycle

- [Require Setup Before Starting](./recipe-require-setup.md)
- [Run One-Time Setup on Install](./recipe-install-init.md)
- [Bootstrap via Temporary Daemon Chain](./recipe-run-until-success.md)
- [Handle Version Upgrades](./recipe-version-migrations.md)
- [Handle Restore from Backup](./recipe-restore-init.md)

### Daemons & Containers

- [Run Multiple Containers](./recipe-multi-daemon.md)
- [Run a PostgreSQL Sidecar](./recipe-postgresql.md)
- [Run a MySQL/MariaDB Sidecar](./recipe-mysql.md)
- [Run a Redis/Valkey Cache](./recipe-valkey.md)
- [Create Dynamic Daemons](./recipe-dynamic-daemons.md)
- [Run a One-Shot Command](./recipe-oneshot.md)
- [Run a Nested OCI Runtime](./recipe-nested-oci-runtime.md)

### Networking

- [Expose a Web UI](./recipe-web-ui.md)
- [Expose Multiple Interfaces](./recipe-multi-interface.md)
- [Expose an API-Only Interface](./recipe-api-interface.md)

### Dependencies

- [Depend on Another Service](./recipe-dependency.md)
- [Enforce Settings on a Dependency](./recipe-enforce-dependency.md)
- [Mount Volumes from Another Service](./recipe-mount-dependency.md)
- [Let Dependents Through a Proxy Auth Gate](./recipe-dependent-auth.md)
- [Support Alternative Dependencies](./recipe-alternative-deps.md)

### Data & Health

- [Back Up and Restore Data](./recipe-backups.md)
- [Add Standalone Health Checks](./recipe-health-checks.md)

### User Communication

- [Post a Notification to the User](./recipe-notification.md)

## Hosting a Registry

- [Overview](./host-registry.md) - What a registry is and why you might run your own
- [Setup](./host-registry-setup.md) - Install and configure the registry service
- [Administration](./host-registry-administration.md) - Day-to-day registry administration with `start-cli`

## Reference

- [Project Structure](./project-structure.md) - Understand the file layout of a StartOS package
- [Manifest](./manifest.md) - Define your service metadata and release notes
- [Versions](./versions.md) - Handle install, update, and downgrade logic
- [Main](./main.md) - Configure daemons, health checks, and the service lifecycle
- [Initialization](./init.md) - Run code when your service initializes
- [Interfaces](./interfaces.md) - Expose network interfaces to users
- [HTTP Proxy Plugins](./http-proxy-plugins.md) - Publish profiles and process attached HTTP traffic
- [Service-to-Service Networking](./service-to-service.md) - Reach another service at runtime
- [Outbound Network Access](./outbound-networking.md) - Reach the internet from your service
- [Actions](./actions.md) - Define user-facing buttons and scripts
- [Tasks](./tasks.md) - Prompt users to run actions at the right time
- [Notifications](./notifications.md) - Post messages to the StartOS notifications panel
- [File Models](./file-models.md) - Represent and validate configuration files
- [Dependencies](./dependencies.md) - Declare and configure service dependencies
- [Makefile](./makefile.md) - Automate build and install workflows
- [Writing READMEs](./writing-readmes.md) - Write effective service documentation
- [Writing Instructions](./writing-instructions.md) - Write the Instructions tab users see in StartOS
- [Publishing](./publishing.md) - Distribute your package through a registry
- [CLI Reference](./cli.md) - The `start-cli` commands most relevant to service developers
