# brochure

The public **Start9 Marketplace** website — [marketplace.start9.com](https://marketplace.start9.com).

Despite the historical name "brochure," this is not a static landing page. It is a single-page Angular app that lets anyone browse the packages published to Start9's registries (and any arbitrary registry you point it at) without owning a server. It reuses the exact same `@start9labs/marketplace` UI that ships inside StartOS, so what you see here matches what a server owner sees in their marketplace tab.

`projects/start-docs/landing` is the documentation site's landing page, not this app.

## Where it lives

This is a thin Angular application project inside the monorepo's single Angular workspace, rooted at the repo root:

- App source: `projects/brochure-marketplace/src/`
- Workspace + build config: `angular.json` (project `brochure-marketplace`) and `package.json`, both at the repo root
- Shared libraries it consumes: `@start9labs/marketplace` (`shared-libs/ts-modules/marketplace`) and `@start9labs/shared` (`shared-libs/ts-modules/shared`)

Run all `npm` scripts from the repo root, not from this directory.

## Quickstart

```bash
npm ci                     # install workspace deps (run once)
npm run build:deps         # build @start9labs/start-core + patch-db client (run once)

npm run start:brochure     # dev server on http://localhost:8200
```

The dev build uses `MockApiService` (fixtures in `src/app/services/api.fixures.ts`) so it runs with no live registry. Production builds swap in `LiveApiService`, which fetches real registries over RPC.

## Build

```bash
npm run build:brochure     # production build -> projects/brochure-marketplace/dist/raw/brochure-marketplace
```

## Deploy

Merges to `master` that touch `projects/brochure-marketplace/**` (or its shared deps) auto-deploy to marketplace.start9.com via `.github/workflows/deploy-brochure.yml`. There is no manual deploy step.

## More

- [ARCHITECTURE.md](ARCHITECTURE.md) — structure, services, data flow
- [AGENTS.md](AGENTS.md) — build/test/format workflow
