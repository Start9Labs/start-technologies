# StartOS Web

Shared TypeScript modules for the monorepo. This directory (`shared-libs/ts-modules`) contains two [Angular](https://angular.dev/) / [Taiga UI](https://taiga-ui.dev/) libraries and a non-Angular core library. The Angular libraries are part of the shared workspace (whose root config files `angular.json`, `package.json`, `tsconfig.json` are at the repo root). The individual app projects live in their product directories and reference these libs.

## Libraries (in this directory)

- **`shared/`** — `@start9labs/shared`: API clients, common components, directives, pipes, services, types, and i18n shared by all apps.
- **`marketplace/`** — `@start9labs/marketplace`: service-discovery / marketplace UI, shared between the StartOS UI and the public marketplace.

- **`start-core/`** — `@start9labs/start-core`: core types, ABI, effects, and OS bindings. Built with its own Makefile, consumed directly by web and bundled into the SDK.

## App projects (defined at the repo root)

`angular.json` declares these applications; their `root`/`sourceRoot` point into the product directories:

- **ui** — primary StartOS admin interface — `../../projects/start-os/web/ui`
- **setup-wizard** — initial-setup UI (`start.local`) — `../../projects/start-os/web/setup-wizard`
- **start-tunnel** — StartTunnel VPN/forwarding management UI — `../../projects/start-tunnel/web`
- **start-wrt** — StartWRT router management UI (embedded into the `startwrt` binary) — `../../projects/start-wrt/web`
- **brochure-marketplace** — public marketplace front (marketplace.start9.com); auto-deploys on merge to `master` — `../../projects/brochure-marketplace`

These apps consume the `shared` lib (and, except `start-wrt`, `marketplace`) from this workspace.

## Quickstart

From the repo root (the Angular workspace is rooted there):

```sh
npm ci
npm run build:deps      # builds @start9labs/start-core + patch-db client (file: deps)
cp shared-libs/ts-modules/config-sample.json config.json
npm run start:ui        # mock-backed dev server
```

## Documentation

See [AGENTS.md](AGENTS.md) for setup, live-server proxying, translations, and operating rules; [ARCHITECTURE.md](ARCHITECTURE.md) describes the structure.
