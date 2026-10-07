# Routing

- Routes are flat arrays of **lazy default exports**: `loadComponent: () => import('./routes/x')`
  (no `.then` — components are `export default class`), `loadChildren: () => import('./x/routes')`
  where the child file ends `export default [ … ] satisfies Routes`. Eager `component:` only for
  shells.
- **Never `providers` on a route.** A subtree that needs scoped services gets a shell component
  (`component: Outlet` wrapping `<router-outlet />` with the providers) — see start-wrt
  `devices/`.
- Guards: **inline `canMatch` arrows** — `canMatch: [() => inject(AuthService).authenticated()]`;
  start-wrt's four same-path `''` routes discriminate purely by `canMatch` (wizard/setup/app/
  login). Class guards are legacy. Dashboards use none at all — an `AdminShell` component gates
  by `adminService.token()`.
- Titles where they matter (public sites): per-route `title:` + a `TitleStrategy` subclass
  ("StartTunnel – X"), or StartOS's `titleResolver` composing "server — page". Embedded UIs
  skip titles.
- `{ path: '**', redirectTo: … }` at every level (wildcard, not `''`+`pathMatch`).
- **A URL that has left the app is permanent.** Emails, pushes, issues and pasted links keep
  the path they were built with, so a renamed route keeps its old path as a redirect
  (`{ path: 'c/:id', redirectTo: 'chat/:id' }`), and every place that builds the URL — server,
  mock, tests — moves in the same change.
- Nav highlights are `routerLinkActive`, never an `active` input computed from the URL. A link
  to an empty path is active on every page under it, so a section's landing page gets a real
  path (`/staff/chat`) and the level's `**` redirects there.
- Navigation: `routerLink` in templates (with `[queryParams]`, `[state]`), `Router.navigate` in
  TS; shareable UI state lives in **query params**, synced bidirectionally (see components.md, templates).
