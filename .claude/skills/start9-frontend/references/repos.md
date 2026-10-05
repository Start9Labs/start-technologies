# Per-repo deltas & commands

- **start-technologies (monorepo)** — workspace root = repo root; `npm ci` then
  `npm run build:deps` **must** run before anything type-checks. Dev: `npm run start:ui`
  (mocks; needs `config.json` copied from `shared-libs/ts-modules/config-sample.json`),
  `start:wrt`, etc. Verify: `npm run check` / `check:<app>` + `make web-format` +
  `npm run check:i18n`. UIs are embedded into Rust binaries at compile time — web build
  precedes cargo. `brochure-marketplace` **auto-deploys to marketplace.start9.com on merge to
  `master`**.
- **start9-store** — SSR; browser APIs only via tokens/`afterNextRender`;
  `TUI_MEDIA.mobile: 1120` is measured — adding a nav item means re-measuring; light theme is a
  deliberate deferral of design; checkout is a redirect in Phase 1 (Shopify) — multi-step
  checkout is Phase 2 (Vendure).
- **ops-server** — admin-only actions check `adminService.isAdmin()`. Never commit `.env`.
- **support-server** (`portal/web/`) — the support portal, not a dashboard: one
  app with the customer chat at `/` and the staff inbox at `/staff`, sharing components and a
  store base and never branching on the role outside the shell and the route guards.
  Same-origin RPC and socket.io, only through `ApiService` and `RealtimeService`; the contract
  is the repo's `portal/web/ARCHITECTURE.md`. No environments — `ng serve` proxies to `mock/`,
  an Express + socket.io server that is the dev backend (there is no `MockApiService`; extend
  the mock). Inline `canMatch` guards on `SessionService.user()` and its `role`. Both themes: the account's
  appearance setting drives `TUI_DARK_MODE` (`provideTaiga()` with no `mode` seeds from the OS,
  StartOS tokens for dark). Local i18n machinery with `en.ts` only. No commit hook:
  `npm run check` runs the compiler, `check-i18n`, the logic tests in `scripts/*.test.ts`
  (`tsx --test`), the mock's type-check, and `prettier --check`. Its `web/AGENTS.md` carries
  the portal-specific rules.
