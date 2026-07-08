# Writing Service READMEs

Every StartOS package README should document **how your service on StartOS differs from the upstream version**. Users should be able to read your README and understand exactly what is different -- everything else, they can find in the upstream docs.

## Guiding Principles

**Do not duplicate upstream documentation.** If something is not mentioned in your README, users should assume the upstream docs are accurate.

Write for two audiences:

1. **Humans** -- clear, scannable, with practical examples
2. **AI assistants** -- structured data that can be parsed programmatically

## Recommended Structure

````markdown
<p align="center">
  <img src="icon.svg" alt="[Service Name] Logo" width="21%">
</p>

# [Service Name] on StartOS

> **Upstream docs:** <https://docs.example.com/>
>
> Everything not listed in this document should behave the same as upstream
> [Service Name]. If a feature, setting, or behavior is not mentioned here,
> the upstream documentation is accurate and fully applicable.

[Brief description of what the service does and link to upstream repo]

---

## Table of Contents

[Links to each section — must include all sections present in the README]

---

## Image and Container Runtime

[Image source, architectures, entrypoint modifications]

## Volume and Data Layout

[Mount points, data directories, StartOS-specific files like store.json]

## Installation and First-Run Flow

[How setup differs from upstream -- skipped wizards, auto-configuration, initial credentials]

## Configuration Management

[Which settings are managed by StartOS vs configurable via upstream methods]

## Network Access and Interfaces

[Exposed ports, protocols, access methods]

## Actions (StartOS UI)

[Each action: name, purpose, availability, inputs/outputs]

## Backups and Restore

[What's backed up, restore behavior]

## Health Checks

[Endpoint, grace period, messages]

## Dependencies

[Required and optional dependencies — version constraints, health checks, mounted volumes, purpose]

## Limitations and Differences

[Numbered list of key limitations compared to upstream]

## What Is Unchanged from Upstream

[Explicit list of features that work exactly as documented upstream]

## Contributing

[Link to AGENTS.md]

---

## Quick Reference for AI Consumers

```yaml
package_id: string
architectures: [list]
volumes:
  volume_name: mount_path
ports:
  interface_name: port_number
dependencies: [list or "none"]
startos_managed_env_vars:
  - VAR_NAME
actions:
  - action-id
```

> [!IMPORTANT]
> Do not include `upstream_version`, image tags, or dependency version constraints in the YAML block (or anywhere else in the README). The manifest is the single source of truth for versions — README version references go stale on every bump and create misinformation. If a user wants to know the version, they can look at the manifest or the service page.
````

## Sections to Document

### Logo

Every README should begin with the service icon centered above the title. Use the standard format:

```html
<p align="center">
  <img src="icon.svg" alt="[Service Name] Logo" width="21%" />
</p>
```

Adjust the `src` to match the actual icon filename (e.g., `icon.png` if the icon is a PNG).

### Image and Container Runtime

| What to Document | Example                                   |
| ---------------- | ----------------------------------------- |
| Image source     | Upstream unmodified, or custom Dockerfile |
| Architectures    | x86_64, aarch64, riscv64                  |
| Entrypoint       | Default or custom                         |

### Volume and Data Layout

| What to Document | Example                              |
| ---------------- | ------------------------------------ |
| Volume names     | `main`, `data`, `config`             |
| Mount points     | `/data`, `/config`                   |
| StartOS files    | `store.json` for persistent settings |
| Database         | Embedded SQLite vs external          |

### Installation and First-Run Flow

Document if your package:

- Skips an upstream setup wizard
- Auto-generates credentials
- Pre-configures settings
- Creates tasks for initial setup

### Configuration Management

Use a table to clarify the division of responsibility:

| StartOS-Managed                          | Upstream-Managed                              |
| ---------------------------------------- | --------------------------------------------- |
| Settings controlled via actions/env vars | Settings configurable via app's own UI/config |

### Actions

For each action, document:

- **Name**: What users see in the StartOS UI
- **Purpose**: What it does
- **Visibility**: Visible, hidden, or conditional
- **Availability**: Any status, only running, only stopped
- **Inputs**: What users provide
- **Outputs**: Credentials, confirmation, etc.

### Network Interfaces

For each interface:

- Port number
- Protocol (HTTP, SSH, etc.)
- Purpose (UI, API, etc.)
- Access methods (LAN IP, .local, .onion, custom domains)

### Backups

- What volumes/data are included
- Data NOT backed up (if any)
- Restore behavior

### Health Checks

- Endpoint or method
- Grace period
- Success/failure messages

### Dependencies

For each dependency, document:

- **Service name** and whether it is required or optional
- **Version constraint** (e.g. `>= 28.3`)
- **Health checks** that must pass before this service starts
- **Mounted volumes** — if a dependency volume is mounted, note the mount point and whether it is read-only
- **Purpose** — why this dependency is needed (e.g. "blockchain data via RPC", "Electrum lookups")

If the service has no dependencies, state "None" explicitly.

### Limitations

Be explicit about:

- Features that do not work or work differently
- Unavailable configuration options
- Unsupported dependencies
- Version-specific limitations

### AI Quick Reference

End every README with a YAML block for machine parsing. This block should contain the package ID, upstream version, image, architectures, volumes, ports, dependencies, managed environment variables, and action IDs.

## Pre-Publish Checklist

- [ ] Centered logo header at the top of the file
- [ ] Upstream docs linked at the top
- [ ] All volumes and mount points documented
- [ ] All actions documented with their purpose
- [ ] All StartOS-managed settings/env vars listed
- [ ] All dependencies documented (or "None" stated explicitly)
- [ ] All limitations listed explicitly
- [ ] "What Is Unchanged" section included
- [ ] YAML quick reference block for AI consumers
- [ ] No specific version numbers anywhere (image tags, upstream version, dep version constraints — all stale the moment you bump)
- [ ] Tested that documented features match actual behavior
