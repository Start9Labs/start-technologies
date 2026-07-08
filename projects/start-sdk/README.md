# Start SDK

The Start SDK (`@start9labs/start-sdk`) is a TypeScript framework for packaging services to run on [StartOS](https://github.com/Start9Labs/start-technologies). It provides a strongly-typed, builder-pattern API for defining every aspect of a service package: manifests, daemons, health checks, networking interfaces, actions, backups, dependencies, configuration, and more.

It lives at `projects/start-sdk/` inside the [start-technologies monorepo](https://github.com/Start9Labs/start-technologies) and is published to npm as [`@start9labs/start-sdk`](https://www.npmjs.com/package/@start9labs/start-sdk).

## Features

- **Type-safe manifest definitions** - Declare your service's identity, metadata, images, volumes, and dependencies with full TypeScript inference.
- **Daemon management** - Define multi-process topologies with startup ordering, ready probes, and graceful shutdown via `Daemons.of().addDaemon()`.
- **Health checks** - Built-in checks for TCP port listening, HTTP(S) endpoints, and custom scripts, with configurable polling strategies (fixed interval, cooldown, adaptive).
- **Network interfaces** - Bind ports, export UI/API/P2P interfaces, and manage hostnames with MultiHost and ServiceInterfaceBuilder.
- **User actions** - Create interactive operations with validated form inputs (text, select, toggle, list, union/variants, and more) that users can trigger from the StartOS UI.
- **Backup and restore** - Rsync-based volume backups with exclude patterns and custom sync paths.
- **Dependency management** - Declare inter-service dependencies with version ranges, health check requirements, and volume mounts.
- **Configuration file helpers** - Read, write, and merge JSON, YAML, TOML, INI, and ENV files with type-safe `FileHelper`.
- **Reactive subscriptions** - Watch for changes to container IPs, SSL certificates, SMTP config, service status, and more with `const()`, `once()`, `watch()`, `onChange()`, and `waitFor()` patterns.
- **Extended versioning (ExVer)** - Flavor-aware semantic versioning with range matching, supporting independent upstream and downstream version tracking.
- **Internationalization** - Built-in i18n support with locale fallback and parameter substitution.
- **Container execution** - Run commands in subcontainers with volume mounts, environment variables, and entrypoint overrides.
- **Plugin system** - Extensible plugin architecture (e.g. `url-v0` for URL management).

## Quick Start

```typescript
import { setupManifest, buildManifest } from '@start9labs/start-sdk'

const manifest = setupManifest({
  id: 'my-service',
  title: 'My Service',
  license: 'MIT',
  // ...
})

export default buildManifest(manifest)
```

The primary entry point is the `StartSdk` facade:

```typescript
import { StartSdk } from '@start9labs/start-sdk'
import { manifest } from './manifest'

export const sdk = StartSdk.of().withManifest(manifest).build(true)
```

From there, `sdk` exposes the full toolkit:

```typescript
// Define daemons
export const main = sdk.setupMain(async ({ effects }) =>
  sdk.Daemons.of(effects).addDaemon('primary', {
    /* ... */
  }),
)

// Define actions
export const setName = sdk.Action.withInput('set-name' /* ... */)

// Define interfaces
export const setInterfaces = sdk.setupInterfaces(async ({ effects }) => {
  const multi = sdk.MultiHost.of(effects, 'web')
  const origin = await multi.bindPort(80, { protocol: 'http' })
  const ui = sdk.createInterface(effects, { name: 'Web UI', id: 'ui' /* ... */ })
  return [await origin.export([ui])]
})

// Define backups
export const { createBackup, restoreBackup } = sdk.setupBackups(async () => sdk.Backups.ofVolumes('main'))
```

## Packages

| Source                               | npm                      | Description                                                                                         |
| ------------------------------------ | ------------------------ | --------------------------------------------------------------------------------------------------- |
| `projects/start-sdk/lib/`            | `@start9labs/start-sdk`  | Full SDK for service developers (the one package you install)                                       |
| `shared-libs/ts-modules/start-core/` | `@start9labs/start-core` | Core types, ABI definitions, and effects interface — bundled into the SDK, not published separately |

The published package also ships `s9pk.mk` and `tsconfig.base.json` at its root, so a service package consumes the canonical build plumbing by reference: `include node_modules/@start9labs/start-sdk/s9pk.mk` in its `Makefile` and `"extends": "@start9labs/start-sdk/tsconfig.base.json"` in its `tsconfig.json`.

## Documentation

For comprehensive packaging guides, tutorials, and API reference:

**[docs.start9.com/packaging](https://docs.start9.com/packaging)**

The packaging docs cover:

- Environment setup and prerequisites
- Project structure and conventions
- Manifest, main, interfaces, actions, and all other service modules
- File models and configuration management
- Versioning, migrations, and initialization
- Dependencies and cross-service communication
- Building and installing `.s9pk` packages

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for environment setup, building from source, testing, and development workflow.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for a detailed overview of the SDK's internal structure, module responsibilities, and data flow.

For agent/dev instructions, see [AGENTS.md](AGENTS.md) (`CLAUDE.md` is a one-line `@AGENTS.md` import).

## License

MIT
