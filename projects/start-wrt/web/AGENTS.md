# AGENTS.md

Angular + Taiga UI frontend for StartWRT. Assumes you've read the parent
[`../AGENTS.md`](../AGENTS.md) — this web app is the `start-wrt` project in the **root Angular
workspace** (it shares the root `package.json`/`node_modules`/`tsconfig.json` and upgrades in
lockstep with the other apps). Build/serve/check it from the repo root: `npm run build:wrt`,
`npm run start:wrt`, `npm run check:wrt`. It uses `@start9labs/shared` for `RELATIVE_URL`,
`pauseFor`, the markdown pipe, and the `CaWizard` login screen. Its strings still come from the
local dictionaries; `i18nService.setLangLocal` also loads the shared dictionary, so a shared
component renders in the router's language — keep new language paths going through it. It deliberately keeps its own HTTP/RPC/connection stack
(`HttpService`/`RpcService`/`ConnectionService`): the aborting per-request timeout that surfaces a
code-0 network error drives the reconnect UX and differs from shared's non-aborting timeout, so
don't swap it for shared's `HttpService`. Error surfacing is bespoke too — `ActionService`/
`FormService` route network drops into the global reconnect indicator with per-action copy rather
than the shared `ErrorService`. `WorkspaceConfig` (start-wrt's flat `config.json`), the WebSocket
progress types, and the i18n-routed `validation-errors` provider also stay local where the shared
shapes don't fit.

## Operating rules

- **Follow the `start9-frontend` skill** at the repo root (`.claude/skills/start9-frontend/`) — the house style for all Start9 Angular/Taiga work: components, styling, forms, overlays, state, the antipattern catalog, and a verified Taiga 5 reference. Read it before writing frontend code; where this file, neighbours, or older docs disagree with the skill, the skill wins.
- **Pattern-match this app's structure.** `routes/published-ports/` is the reference route folder (`index.ts` page + `table.ts` + `dialog.ts` + `service.ts`).
