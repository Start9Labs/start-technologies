# StartOS Web

The single [Angular](https://angular.dev/) + TypeScript workspace (Angular 22, [Taiga UI 5](https://taiga-ui.dev/)) shared by every Start9 front-end. This directory (`shared-libs/ts-modules`) contains the two shared libraries and is part of the Angular workspace (whose root config files `angular.json`, `package.json`, `tsconfig.json` are at the repo root). The individual app projects live in their product directories and reference these libs.

## Libraries (in this directory)

- **`shared/`** — `@start9labs/shared`: API clients, common components, directives, pipes, services, types, and i18n shared by all apps.
- **`marketplace/`** — `@start9labs/marketplace`: service-discovery / marketplace UI, shared between the StartOS UI and the public marketplace.

## App projects (defined here, rooted elsewhere)

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
npm run build:deps      # builds start-sdk + patch-db client (file: deps)
cp shared-libs/ts-modules/config-sample.json config.json
npm run start:ui        # mock-backed dev server
```

## Documentation

See [CONTRIBUTING.md](CONTRIBUTING.md) for full setup, live-server proxying, and translation guides, [ARCHITECTURE.md](ARCHITECTURE.md) for how the front end is structured, and [AGENTS.md](AGENTS.md) for agent / day-to-day operating rules.
