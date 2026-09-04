# AGENTS.md — brochure

Practical instructions for agents working on the public Start9 Marketplace web app (marketplace.start9.com). CLAUDE.md is a one-line `@AGENTS.md` import. See [ARCHITECTURE.md](ARCHITECTURE.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

**Read up the tree first.** These docs are hierarchical: before working here, read the `AGENTS.md` in each enclosing directory up to the repo root (and their `ARCHITECTURE.md` / `CONTRIBUTING.md` where relevant). This file covers only what is specific to this scope and does not repeat rules already stated higher up.

## What this is

A single-page Angular 22 app that wraps the shared `@start9labs/marketplace` library to browse package registries. It is **not** the marketing landing page (that's `projects/start-docs/landing`), despite the "brochure" name. It is a **public website** that **auto-deploys on merge to `master`** via `.github/workflows/deploy-brochure.yml` — unlike `ui`, `setup-wizard`, and `start-tunnel`, which ship inside the OS image.

## Layout

- App source: `projects/brochure-marketplace/src/` (this directory).
- Build/lint/serve config: `angular.json` (project `brochure-marketplace`) at repo root — **not** here.
- npm scripts: `package.json` at repo root. Run them from the repo root.
- `projects/brochure-marketplace/tsconfig.json` extends the repo root `tsconfig.json` and only adds path mappings; the real compiler settings live there.
- It consumes `@start9labs/marketplace` (`shared-libs/ts-modules/marketplace`) and `@start9labs/shared` (`shared-libs/ts-modules/shared`). Most UI building blocks already exist in those libs — reuse before reinventing.

## Build & test (run from the repo root)

```bash
npm ci                 # once
npm run build:deps     # once: builds @start9labs/start-core + patch-db client (required)

npm run start:brochure # dev server, http://localhost:8200, uses MockApiService
npm run build:brochure # production build -> projects/brochure-marketplace/dist/raw/brochure-marketplace
npm run check:brochure # tsc --noEmit type check for this project
make web-format        # prettier across web projects incl. brochure (run from repo root)
```

Run `npm run check:brochure` and `make web-format-check` before pushing changes here.

## Gotchas

- **Dev vs prod data source.** `app.config.ts` picks the API impl from `environment.production`: dev → `MockApiService` (fixtures in `src/app/services/api.fixures.ts`), prod → `LiveApiService` (real RPC). When testing data-shape changes, update the fixtures or you'll see stale/empty results in dev.
- **`build:deps` is a prerequisite.** The app resolves `@start9labs/start-core` from `shared-libs/ts-modules/start-core/dist` and transitively needs the patch-db client. A fresh checkout that skips `build:deps` fails the brochure build/type-check.
- **Registry state lives in `MarketplaceService`** (`src/app/services/marketplace.service.ts`), which extends `AbstractMarketplaceService` from the shared lib. Custom registries persist in `localStorage` under the `_startos/` prefix. Don't add a parallel state store.
- **The `start9-frontend` skill** at the repo root (`.claude/skills/start9-frontend/`) carries all Angular/Taiga house style, including mandatory i18n — read it before UI work; the skill wins over older docs and existing code.
- **Public deploy.** A merge to `master` touching `projects/brochure-marketplace/**` (or its shared deps) ships to production immediately. Test the production build locally before merging.

## Scope

Edit only `projects/brochure-marketplace/`. Workspace-wide concerns (shared libs, angular.json, SDK) belong to their own projects — coordinate there. Don't edit `CLAUDE.md` (it's just `@AGENTS.md`).
