# AGENTS.md

Angular + Taiga UI frontend for StartWRT. Assumes you've read the parent
[`../AGENTS.md`](../AGENTS.md) — this web app is the `start-wrt` project in the **root Angular
workspace** (it shares the root `package.json`/`node_modules`/`tsconfig.json` and upgrades in
lockstep with the other apps). Build/serve/check it from the repo root: `npm run build:wrt`,
`npm run start:wrt`, `npm run check:wrt`. It uses `@start9labs/shared` for `RELATIVE_URL`,
`pauseFor`, and the markdown pipe. It deliberately keeps its own HTTP/RPC/connection stack
(`HttpService`/`RpcService`/`ConnectionService`): the aborting per-request timeout that surfaces a
code-0 network error drives the reconnect UX and differs from shared's non-aborting timeout, so
don't swap it for shared's `HttpService`. Error surfacing is bespoke too — `ActionService`/
`FormService` route network drops into the global reconnect indicator with per-action copy rather
than the shared `ErrorService`. `WorkspaceConfig` (start-wrt's flat `config.json`), the WebSocket
progress types, and the i18n-routed `validation-errors` provider also stay local where the shared
shapes don't fit.

## Operating rules

- **Taiga-first.** Use Taiga components, directives, and APIs whenever possible. Don't hand-roll HTML/CSS unless nothing in Taiga fits.
- **When unsure about a Taiga API, look it up.** Don't guess component names, inputs, or directives — see CONTRIBUTING.md § Taiga UI Lookup for the WebFetch URL and MCP server config.
- **Pattern-match.** Search the codebase for an analogous existing route/component before writing new code. Copy the conventions used in neighbours (e.g. `routes/published-ports/` for the table + dialog pattern).
- **No test framework is wired up here** (no Jest/Karma/Vitest in the workspace). Don't add one without asking.

## Taiga 5 idioms to default to

Patterns a senior Taiga refactor corrected in our own code — exactly what "pattern-match the neighbours" gets wrong while the codebase is mid-migration. Default to these; verify specifics against the MCP/docs.

- **`tuiProvide(TOKEN, Class)`** (`@taiga-ui/cdk`) — a _type-safe_ wrapper around `{ provide: TOKEN, useExisting: Class }` (checks `Class` satisfies `TOKEN`). Use it for every `useExisting` binding.
- **DI: only root and node injectors.** App-wide singletons at **root** (`app.config.ts`); everything else on the **node** (a component's `providers`/`viewProviders`). **Never add `providers` to routes** — lazy routes spin up confusing semi-root injectors.
- **A component that _is_ a control** (button/link/badge) → attribute selector (`a[myThing]`) + `hostDirectives: [TuiButton]` + static attrs in `host: {}` + config via option providers (`tuiButtonOptionsProvider`, `TUI_ICON_START`), styled on `:host`. Never a `<my-thing>` wrapper around a styled child with a `:host { display: grid }` stretch hack.
- **Layout primitives over CSS**: `tuiHeader`, `tuiForm`, `tuiList`, `tuiAvatar` (large icons — not `tui-icon` + `font-size`), `tuiBadge` (small action chips). Hand-centering a card's contents means you missed a primitive.
- **`appearance="positive|warning|…"`** on components, not `class="g-positive"`. `g-*` color utilities live only in the shared global stylesheet — consolidated, not duplicated per app.
- **Signals**: `[(ngModel)]="signal"`; `linkedSignal({ source, computation })` to auto-reset a signal; optional inputs are `input<T>()` (not `input<T | null>(null)`), and `*ngTemplateOutlet` no-ops on null — no `@if` guard. `protected readonly` for template-used members; static host attrs (`type: 'button'`) in `host: {}`.
