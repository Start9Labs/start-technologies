# AGENTS.md

Angular + Taiga UI frontend for StartWRT. Assumes you've read the parent
[`../AGENTS.md`](../AGENTS.md) — this web app is the `start-wrt` project in the **root Angular
workspace** (it shares the root `package.json`/`node_modules`/`tsconfig.json` and upgrades in
lockstep with the other apps). Build/serve/check it from the repo root: `npm run build:wrt`,
`npm run start:wrt`, `npm run check:wrt`. It uses `@start9labs/shared` for `RELATIVE_URL`,
`pauseFor`, the markdown pipe, and the `CaWizard` login screen. Its own strings come from the
local dictionaries; `i18nService.setLangLocal` also loads the shared dictionary that shared
components translate through, so change the language only through it. It deliberately keeps its own HTTP/RPC/connection stack
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

## Getting Started

The web is the `start-wrt` project in the root Angular workspace — run everything from the repo root:

```bash
cp projects/start-wrt/web/config-sample.json projects/start-wrt/web/config.json  # One-time: local config
npm ci                                # Install the whole workspace
npm run build:deps                    # Build the file: deps — once after install
npm run start:wrt                     # Dev server with mock API
npm run build:wrt                     # Production build
npm run check:wrt                     # Type-check without emitting
```

### Configuring config.json

`config.json` is **gitignored** — it's generated from `config-sample.json`:

- **Local dev:** `cp config-sample.json config.json`, then edit freely. `npm run start:wrt` runs `build-config.js` first, which stamps the current git hash into `gitHash`.
- **Production / CI:** `make start-wrt` triggers `web/update-config.sh`, which flips `useMocks` to `false` and stamps `gitHash` from `build/env/GIT_HASH.txt`.
- **Demo site:** `npm run build:wrt:demo` stamps `config.json` from `config-sample.json` (`useMocks` stays `true`), builds, and adds `404.html` so a static host boots the app on deep links; `.github/workflows/deploy-startwrt-demo.yml` runs it on every `master` push that touches the UI and publishes the bundle through the `.github/actions/nextexplorer-publish` action to the NextExplorer folder Start9 Pages serves as router-demo.start9.com.

Schema:

```json
{
  "useMocks": true,
  "api": { "url": "rpc", "version": "v1" },
  "gitHash": ""
}
```

- `useMocks: true` — Uses `MockApiService` (no router needed)
- `useMocks: false` — Uses `LiveApiService` (requires running backend)
- `gitHash` — Stamped at build time; available via `WorkspaceConfig.gitHash` for About / diagnostics UIs.

The API URL resolves to `document.location.origin + /rpc/v1`.
