# FRONTEND.md — StartOS setup-wizard

Facts the `start9-frontend` skill needs about the StartOS setup wizard. Adds to the
[repo-root `FRONTEND.md`](../../../../FRONTEND.md); build and deploy are in
[`projects/start-os/AGENTS.md`](../../AGENTS.md).

## Change detection

zone.js (`polyfills` in `angular.json`).

## Theme

Dark, `provideTaiga({ mode: 'dark' })`, on the shared brand sheet.

## i18n

Shared dictionaries.

## Backend

JSON-RPC typed from `@start9labs/start-core`'s `RPC.Setup` and `RPC.StartOS` trees; no PatchDB.

## Verify

`npm run check:setup`, `npm run build:setup`; mock dev server `npm run start:setup`.
