# Summary

[Service Packaging](README.md)

---

# Getting Started

- [Environment Setup](environment-setup.md)
- [Quick Start](quick-start.md)
- [Development Workflow](workflow.md)
- [Agent Context](agent-context.md)

# Recipes

- [Browse All Recipes](recipes.md)

## Configuration

- [Set Up a Basic Service](recipe-basic-service.md)
- [Package a Prebuilt Docker Image](recipe-prebuilt-image.md)
- [Create Configuration Actions](recipe-config-actions.md)
- [Generate Config Files](recipe-config-files.md)
- [Pass Config via Environment Variables](recipe-env-vars.md)
- [Hardcode Config Values](recipe-hardcode-config.md)
- [Set a Primary URL](recipe-primary-url.md)
- [Set Up SMTP / Email](recipe-smtp.md)

## Credentials & Access Control

- [Auto-Generate Internal Secrets](recipe-internal-secrets.md)
- [Prompt User to Create Admin Credentials](recipe-admin-credentials.md)
- [Reset a Password](recipe-reset-password.md)
- [Gate User Registration](recipe-registration-gating.md)

## Setup & Lifecycle

- [Require Setup Before Starting](recipe-require-setup.md)
- [Run One-Time Setup on Install](recipe-install-init.md)
- [Bootstrap via Temporary Daemon Chain](recipe-run-until-success.md)
- [Handle Version Upgrades](recipe-version-migrations.md)
- [Handle Restore from Backup](recipe-restore-init.md)

## Daemons & Containers

- [Run Multiple Containers](recipe-multi-daemon.md)
- [Run a PostgreSQL Sidecar](recipe-postgresql.md)
- [Run a MySQL/MariaDB Sidecar](recipe-mysql.md)
- [Run a Redis/Valkey Cache](recipe-valkey.md)
- [Create Dynamic Daemons](recipe-dynamic-daemons.md)
- [Run a One-Shot Command](recipe-oneshot.md)
- [Run a Nested OCI Runtime](recipe-nested-oci-runtime.md)

## Networking

- [Expose a Web UI](recipe-web-ui.md)
- [Expose Multiple Interfaces](recipe-multi-interface.md)
- [Expose an API-Only Interface](recipe-api-interface.md)

## Dependencies

- [Depend on Another Service](recipe-dependency.md)
- [Enforce Settings on a Dependency](recipe-enforce-dependency.md)
- [Mount Volumes from Another Service](recipe-mount-dependency.md)
- [Support Alternative Dependencies](recipe-alternative-deps.md)

## Data & Health

- [Back Up and Restore Data](recipe-backups.md)
- [Add Standalone Health Checks](recipe-health-checks.md)

## User Communication

- [Post a Notification to the User](recipe-notification.md)

# Hosting a Registry

- [Overview](host-registry.md)
- [Setup](host-registry-setup.md)
- [Administration](host-registry-administration.md)

# Reference

- [Project Structure](project-structure.md)
- [Manifest](manifest.md)
- [Versions](versions.md)
- [Main](main.md)
- [Initialization](init.md)
- [Interfaces](interfaces.md)
- [Service-to-Service Networking](service-to-service.md)
- [Actions](actions.md)
- [Tasks](tasks.md)
- [Notifications](notifications.md)
- [File Models](file-models.md)
- [Dependencies](dependencies.md)
- [Makefile](makefile.md)
- [Writing READMEs](writing-readmes.md)
- [Writing Instructions](writing-instructions.md)
- [Publishing](publishing.md)
- [CLI Reference](cli.md)
