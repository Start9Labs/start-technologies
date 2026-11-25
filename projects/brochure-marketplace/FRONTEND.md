# FRONTEND.md — brochure-marketplace

Facts the `start9-frontend` skill needs about the public marketplace site. Adds to the
[repo-root `FRONTEND.md`](../../FRONTEND.md). Its build, data source, registry state and
public deploy are in [`AGENTS.md`](AGENTS.md).

## Change detection

zone.js (`polyfills` in `angular.json`); legacy.

## Theme

Dark, `provideTaiga({ mode: 'dark' })`, on the shared brand sheet.

## i18n

Shared dictionaries.

## Backend

Registry RPC, called directly. The Live/Mock switch is `environment.production`, not
`config.json` (`AGENTS.md` § Gotchas).

## Verify

`npm run check:brochure`, `npm run build:brochure`; mock dev server `npm run start:brochure`.
