# Updating

Keep StartTunnel up to date using the web UI or the CLI. Run the CLI on your VPS as `root`.

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
