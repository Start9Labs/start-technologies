# Recipes

This is the primary entry point for StartOS service packaging — for both you and your AI coding agent. Each recipe describes a common packaging pattern, names the SDK constructs involved, links to reference pages for API details, and points to real packages for working code. Your agent reads these to understand what to build; you read them to understand what to ask for.

If you're using [Claude Code](https://claude.com/claude-code) (recommended), point your agent at the recipe for your task and let it follow the reference and package links from there.

> **Starting a brand-new package?** Scaffold it first with `start-cli s9pk init-package "My Service"`, then work the generated `TODO.md` from top to bottom — don't hand-assemble files by copying another package. If you're wrapping an existing upstream Docker image (the common case), read [Package a Prebuilt Docker Image](recipe-prebuilt-image.md) before you start.

## Configuration

| Recipe | Description |
|--------|-------------|
| [Set Up a Basic Service](recipe-basic-service.md) | Minimal single-container service with a web UI, health check, and backup |
| [Package a Prebuilt Docker Image](recipe-prebuilt-image.md) | Wrap an upstream `linuxserver/*` or official image — verify the image, mount every data path, expose all ports, handle init systems and credentials |
| [Create Configuration Actions](recipe-config-actions.md) | Let users configure your service through actions with input forms |
| [Generate Config Files](recipe-config-files.md) | Produce YAML, TOML, INI, JSON, or ENV files from user settings using FileModel |
| [Pass Config via Environment Variables](recipe-env-vars.md) | Configure your service through environment variables in the daemon definition |
| [Hardcode Config Values](recipe-hardcode-config.md) | Lock down ports, paths, or auth modes so users cannot change them |
| [Set a Primary URL](recipe-primary-url.md) | Let users choose which hostname the service uses for links, invites, and federation |
| [Set Up SMTP / Email](recipe-smtp.md) | Let users configure email sending with disabled/system/custom modes |

## Credentials & Access Control

| Recipe | Description |
|--------|-------------|
| [Auto-Generate Internal Secrets](recipe-internal-secrets.md) | Generate passwords or tokens in init for internal use (database auth, secret keys) |
| [Prompt User to Create Admin Credentials](recipe-admin-credentials.md) | Critical task that points to a "set admin password" action — the action generates, stores, and returns the credential on each invocation (first-set + rotation) |
| [Reset a Password](recipe-reset-password.md) | Action that regenerates credentials and updates the running application |
| [Gate User Registration](recipe-registration-gating.md) | Toggle action that enables/disables public signups with a dynamic label |

## Setup & Lifecycle

| Recipe | Description |
|--------|-------------|
| [Require Setup Before Starting](recipe-require-setup.md) | Block service startup with a critical task until the user completes configuration |
| [Run One-Time Setup on Install](recipe-install-init.md) | Generate passwords, seed databases, or bootstrap config on first install only |
| [Bootstrap via Temporary Daemon Chain](recipe-run-until-success.md) | Start the service during init, call its API to bootstrap, then tear it down |
| [Handle Version Upgrades](recipe-version-migrations.md) | Migrate data between package versions using the version graph |
| [Handle Restore from Backup](recipe-restore-init.md) | Re-register services or fix state after restoring from backup |

## Daemons & Containers

| Recipe | Description |
|--------|-------------|
| [Run Multiple Containers](recipe-multi-daemon.md) | App + database, app + cache, app + worker — multi-daemon setups |
| [Run a PostgreSQL Sidecar](recipe-postgresql.md) | Password generation, pg_isready health check, pg_dump backup |
| [Run a MySQL/MariaDB Sidecar](recipe-mysql.md) | MySQL daemon, health check, mysqldump backup and restore |
| [Run a Redis/Valkey Cache](recipe-valkey.md) | Ephemeral cache daemon with valkey-cli ping health check |
| [Create Dynamic Daemons](recipe-dynamic-daemons.md) | Variable number of daemons based on user configuration |
| [Run a One-Shot Command](recipe-oneshot.md) | Migrations, file ownership fixes, or setup scripts before the main daemon starts |
| [Run a Nested OCI Runtime](recipe-nested-oci-runtime.md) | Rootless Podman or Docker inside the service for CI runners, build daemons, sandboxed jobs |

## Networking

| Recipe | Description |
|--------|-------------|
| [Expose a Web UI](recipe-web-ui.md) | Single HTTP interface for browser access |
| [Expose Multiple Interfaces](recipe-multi-interface.md) | RPC, API, peer, WebSocket, or SSH on different ports |
| [Expose an API-Only Interface](recipe-api-interface.md) | Programmatic access with no browser UI |
| [Reach Another Service](service-to-service.md) | Dial a dependency over the host bridge (`getOsIp` + assigned port) |

## Dependencies

| Recipe | Description |
|--------|-------------|
| [Depend on Another Service](recipe-dependency.md) | Declare a dependency, read its connection info, and auto-configure |
| [Enforce Settings on a Dependency](recipe-enforce-dependency.md) | Create a cross-service task that requires specific dependency configuration |
| [Mount Volumes from Another Service](recipe-mount-dependency.md) | Read-only access to a dependency's data volume |
| [Support Alternative Dependencies](recipe-alternative-deps.md) | Let users choose between backends (e.g., LND vs CLN) |

## Data & Health

| Recipe | Description |
|--------|-------------|
| [Back Up and Restore Data](recipe-backups.md) | Volume snapshots, pg_dump, mysqldump, and incremental rsync strategies |
| [Add Standalone Health Checks](recipe-health-checks.md) | Sync progress, reachability, and other ongoing checks beyond daemon readiness |

## User Communication

| Recipe | Description |
|--------|-------------|
| [Post a Notification to the User](recipe-notification.md) | Send a plain or markdown-detailed notification to the StartOS panel when a long-running action finishes or a sync completes |
