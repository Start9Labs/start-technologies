# Updating

Keep StartTunnel up to date using the web UI, CLI, or install script.

## Web UI

Navigate to **Settings > Version > Check for Updates**. After the update finishes, reload any open StartTunnel page. When updating an interface that predates automatic revalidation, perform one hard refresh to load the current interface.

## CLI

Check for updates:

```bash
start-tunnel update check
```

Install the latest version:

```bash
start-tunnel update apply
```

## Install script

Re-run the install command:

```bash
curl -sSL https://start9.com/start-tunnel/install.sh | sh
```

The installer detects the existing installation, prompts for confirmation, and restarts the service.
