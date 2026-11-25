# Per-repo deltas & commands

Each repo's own `AGENTS.md` carries its project facts, and its `package.json` its stack
versions; read them on the way in. What follows is only what this skill's rules need to know
about each app: change detection, theme, i18n, backend, and repo-specific rules.

- **start-technologies (monorepo)** — workspace root = repo root; `npm ci` then
  `npm run build:deps` **must** run before anything type-checks. Dev: `npm run start:ui`
  (mocks; needs `config.json` copied from `shared-libs/ts-modules/config-sample.json`),
  `start:wrt`, etc. Verify: `npm run check` / `check:<app>` + `make web-format` +
  `npm run check:i18n`. UIs are embedded into Rust binaries at compile time — web build
  precedes cargo. `brochure-marketplace` **auto-deploys to marketplace.start9.com on merge to
  `master`**.
  - StartOS `ui` (`projects/start-os/web/ui`): zone.js, zoneless pending; dark via
    `provideTaiga({mode})`; i18n with shared dictionaries; JSON-RPC + PatchDB push.
  - `setup-wizard` (`projects/start-os/web/setup-wizard`): zone.js; dark; i18n (shared);
    JSON-RPC.
  - `start-tunnel` (`projects/start-tunnel/web`): zoneless; dark; i18n (local dictionaries);
    JSON-RPC + PatchDB.
  - `start-wrt` (`projects/start-wrt/web`): zoneless; light and dark (`TUI_DARK_MODE`); i18n
    (local dictionaries); signed-DH RPC with bootstrap possession, 5s polling.
  - `brochure-marketplace` (`projects/brochure-marketplace`): zone.js (legacy); dark; i18n
    (shared); registry RPC direct.
- **start9-store** (`ops/start9-store/web`) — zoneless; light; no i18n; REST + Zod through
  the `/api` BFF. SSR: browser APIs only through tokens or `afterNextRender`. Its header
  collapses at `TUI_MEDIA.tablet`, not `mobile`, so `tui-root._mobile` applies below 1024px.
- **ops-server** (`ops/ops-server/web`) — zoneless; dark, `#07a4ff`, Montserrat; no i18n;
  REST `/_api`, same-origin Express. Admin-only actions check `adminService.isAdmin()`.
- **support-server** (`ops/support-server/portal/web`) — zoneless; light and dark (theme
  setting) on StartOS tokens; i18n (local dictionaries); RPC `/api` + socket.io, same-origin.
  One app for customers (`/`) and staff (`/staff`) that
  never branches on the role outside the shell and the route guards. There is no
  `MockApiService`: the dev backend is the Express + socket.io server in `mock/`, so extend
  that. Its `web/AGENTS.md` carries the rest.
- **start-equity** (`ops/start-equity/web`) — zoneless; light and dark (OS, `TUI_DARK_MODE`
  toggle); no i18n; REST `/_api`, same-origin Express; `MockApiService` in dev.
- **Helix console** (`helix-startos-internal` `app/web`) — zoneless; light and dark following
  the OS; no i18n; REST `/_api` + WebSocket push, same-origin Express. Imports the server's contract
  types and pure helpers from `app/src/web`. No `MockApiService`: `src/web/browser.test.ts`
  drives the built console against an isolated server, so it locates elements by role and
  accessible name; keep those stable or update the test. Verify: `npm run check:web` and
  `npm run test:browser` from `app/`.
