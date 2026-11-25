# FRONTEND.md — StartTunnel

Facts the `start9-frontend` skill needs about the StartTunnel UI. Adds to the
[repo-root `FRONTEND.md`](../../../FRONTEND.md); build and deploy are in
[`projects/start-tunnel/AGENTS.md`](../AGENTS.md).

## Change detection

Zoneless.

## Theme

Dark, `provideTaiga({ mode: 'dark' })`, on the shared brand sheet.

## i18n

Local dictionaries in `src/app/i18n/`. The app provides its English-to-ID map as `I18N_IDS`,
so its dictionaries carry every shared string it renders. Check: `npm run check:i18n:tunnel`.

## Backend

JSON-RPC typed from `@start9labs/start-core`'s `Tunnel` tree, + PatchDB push. Titles come from
`AppTitleStrategy` ("StartTunnel – X").

## Verify

`npm run check:tunnel`, `npm run build:tunnel`; mock dev server `npm run start:tunnel`.
