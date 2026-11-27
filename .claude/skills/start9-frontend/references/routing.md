# Routing

- A heavy library one route needs loads inside the function that uses it,
  `const { default: ExcelJS } = await import('exceljs')`, so it becomes a chunk of its own. A
  package whose root entry re-exports a heavy module puts it in the initial bundle wherever the
  root is imported; give that module its own subpath export (`@scope/engine/import/<format>`).
- Routes are flat arrays of **lazy default exports**: `loadComponent: () => import('./routes/x')`
  (no `.then` — components are `export default class`), `loadChildren: () => import('./x/routes')`
  where the child file ends `export default [ … ] satisfies Routes`. Eager `component:` only for
  shells.
- **Never `providers` on a route.** A subtree that needs scoped services gets a shell component
  (`component: Outlet` wrapping `<router-outlet />` with the providers).
- Guards: **inline `canMatch` arrows** — `canMatch: [() => inject(AuthService).authenticated()]`;
  same-path `''` routes can discriminate purely by `canMatch` (wizard / setup / app / login).
  Class guards are legacy. An app gated as a whole can skip guards: a shell component gates by
  the auth token signal.
- Titles where they matter: per-route `title:` + a `TitleStrategy` subclass ("Product – X"),
  or a title resolver composing "server — page". Embedded UIs may skip titles.
- `{ path: '**', redirectTo: … }` at every level (wildcard, not `''`+`pathMatch`).
- **A URL that has left the app is permanent.** Emails, pushes, issues and pasted links keep
  the path they were built with, so a renamed route keeps its old path as a redirect
  (`{ path: 'c/:id', redirectTo: 'chat/:id' }`), and every place that builds the URL — server,
  mock, tests — moves in the same change.
- Nav highlights are `routerLinkActive`, never an `active` input computed from the URL. A link
  to an empty path is active on every page under it, so a section's landing page gets a real
  path (`/settings/general`) and the level's `**` redirects there.
- Navigation: `routerLink` in templates (with `[queryParams]`, `[state]`), `Router.navigate` in
  TS; shareable UI state lives in **query params**, synced bidirectionally (see components.md, templates).
- **Route and query params are inputs.** `provideRouter(routes, withComponentInputBinding())`,
  then the routed component declares `id = input('')` for `:id` and `tab = input('')` for
  `?tab`. A child outside the outlet takes the value from that routed component
  (`<aside [appQueue]="tab()">`) rather than injecting `ActivatedRoute` itself.
  `inject(ActivatedRoute).snapshot` is left for a value needed while the class is constructed,
  such as a form group's seed. An absent param sets the input to `undefined`, not its default,
  so supply the fallback where the value is read.
