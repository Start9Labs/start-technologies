# Per-repo deltas & commands

- **start-technologies (monorepo)** — workspace root = repo root; `npm ci` then
  `npm run build:deps` **must** run before anything type-checks. Dev: `npm run start:ui`
  (mocks; needs `config.json` copied from `shared-libs/ts-modules/config-sample.json`),
  `start:wrt`, etc. Verify: `npm run check` / `check:<app>` + `make web-format` +
  `npm run check:i18n`. Never hand-edit `osBindings/*.ts` (regenerate from Rust). UIs are
  embedded into Rust binaries at compile time — web build precedes cargo.
  `brochure-marketplace` **auto-deploys to marketplace.start9.com on merge to `master`**.
  StartWRT web keeps its own HTTP/RPC/connection stack and local i18n dictionaries —
  deliberate; don't "unify" either without the maintainer.
- **start9-store** — SSR; browser APIs only via tokens/`afterNextRender`; `/api/*` only (the
  frontend must never know Shopify/Vendure exists); icons via root `postinstall` (don't move
  into `angular.json` — hoisted-workspace limitation); `TUI_MEDIA.mobile: 1120` is measured —
  adding a nav item means re-measuring; light theme is a deliberate deferral of design;
  checkout is a redirect in Phase 1 (Shopify) — multi-step checkout is Phase 2 (Vendure).
- **ops-server** — same-origin Express serves the build; relative `/_api` URLs, no
  proxy/environments/CORS; dark theme, accent `#07a4ff`, Montserrat. No route guards —
  `AdminShell` gates on `adminService.token()`; admin-only actions check
  `adminService.isAdmin()`; all HTTP through `AdminService` (authed) / `ApiService` (public).
  Husky+lint-staged Prettier on commit — fix formatting, never `--no-verify`. Never commit
  `.env`.
- **support-server** (`web/`) — the customer support portal over Frappe Helpdesk, not a
  dashboard: same-origin `/api/method/start9_support.api.*` (Frappe's `{ message }` envelope,
  `X-Frappe-CSRF-Token` on every POST) plus Frappe's socket.io for live updates; no
  environments — `ng serve` proxies to `web/mock`, an Express + socket.io server that is the
  dev backend (there is no `MockApiService`; extend the mock). Frappe session in the frontend:
  inline `canMatch` guards on `SessionService.user()`. Both themes follow the OS preference
  (`provideTaiga()` with no `mode`, StartOS tokens for dark, no in-app toggle). Local i18n
  machinery with `en.ts` only. No commit hook: `npm run check` runs the compiler,
  `check-i18n`, the mock's type-check, and `prettier --check`. Its `web/AGENTS.md` carries the
  portal-specific rules.
