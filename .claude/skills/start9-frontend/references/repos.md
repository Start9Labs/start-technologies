# Per-repo deltas & commands

Each repo's own `AGENTS.md` carries its project facts; read it on the way in. What follows is
only what this skill's rules need to know about each one.

- **start-technologies (monorepo)** — workspace root = repo root; `npm ci` then
  `npm run build:deps` **must** run before anything type-checks. Dev: `npm run start:ui`
  (mocks; needs `config.json` copied from `shared-libs/ts-modules/config-sample.json`),
  `start:wrt`, etc. Verify: `npm run check` / `check:<app>` + `make web-format` +
  `npm run check:i18n`. UIs are embedded into Rust binaries at compile time — web build
  precedes cargo. `brochure-marketplace` **auto-deploys to marketplace.start9.com on merge to
  `master`**.
- **start9-store** — SSR: browser APIs only through tokens or `afterNextRender`. Its header
  collapses at `TUI_MEDIA.tablet`, not `mobile`, so `tui-root._mobile` applies below 1024px.
- **ops-server** — admin-only actions check `adminService.isAdmin()`.
- **support-server** (`portal/web/`) — one app for customers (`/`) and staff (`/staff`) that
  never branches on the role outside the shell and the route guards. There is no
  `MockApiService`: the dev backend is the Express + socket.io server in `mock/`, so extend
  that. Its `web/AGENTS.md` carries the rest.
- **Helix console** (`helix-startos-internal` `app/web`) — imports the server's contract
  types and pure helpers from `app/src/web`. No `MockApiService`: `src/web/browser.test.ts`
  drives the built console against an isolated server, so it locates elements by role and
  accessible name; keep those stable or update the test. Verify: `npm run check:web` and
  `npm run test:browser` from `app/`.
