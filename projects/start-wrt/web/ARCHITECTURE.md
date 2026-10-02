# Frontend Architecture

Angular single-page application for the StartWRT admin UI. TypeScript, Taiga UI, zoneless change detection, signal-based state, standalone components. No NgModules.

## Project Structure

```
src/app/
├── app.ts                    # Authenticated shell (CSS Grid: header/nav/main/aside), mounted inside the router — TuiRoot is hosted by the bootstrap component in src/main.ts
├── app.config.ts             # Providers: Taiga, routing, API, HTTP, i18n
├── app.icons.ts              # Bundled Taiga icon set (tuiIconsProvider)
├── app.routes.ts             # Top-level routes (auth guard splits login/setup/home)
│
├── components/               # Shared UI components
│   ├── aside.ts              # Right help panel — renders HelpService content via `| markdown | dompurify`
│   ├── copy.ts               # Copy-to-clipboard directive
│   ├── footer.ts             # Form footer (Cancel / Save buttons)
│   ├── form.ts               # [formLoading] directive — TuiForm + TuiCardLarge + TuiSkeleton
│   ├── header.ts             # Top bar: search, help toggle (HELP_OPEN), system info
│   ├── masked.ts             # Mask sensitive text with toggle + copy
│   ├── nav.ts                # Left sidebar navigation
│   ├── placeholder.ts        # Empty-state placeholder with icon
│   ├── schedule.ts           # app-schedule — 7-day schedule-window grid
│   ├── summary.ts            # Read-only display with copy button
│   └── window.ts             # AddWindow dialog — schedule-window form
│
├── help/                     # Contextual help (route-keyed markdown)
│   ├── help.ts               # HelpService (content resolved to active language) + HELP_OPEN signal token
│   ├── modal-help.ts         # ng-template[modalHelp] — help content for dialogs
│   └── content/              # en/es/de/fr/pl — English eager, other locales lazy-loaded
│
├── i18n/                     # UI translation layer
│   ├── i18n.pipe.ts          # {{ 'Key' | i18n }}
│   ├── i18n.providers.ts     # I18N token + lazy dictionary loaders
│   ├── i18n.service.ts       # Active language state
│   ├── validation-errors.ts  # Translated form validation messages
│   └── dictionaries/         # en/es/de/fr/pl — checked by `npm run check:i18n:wrt` (web/scripts/check-i18n.mjs)
│
├── pipes/
│   └── to-camel.pipe.ts      # {{ value | toCamel }}
│
├── services/
│   ├── action.service.ts     # Wraps async ops with loading/success/error toasts
│   ├── auth.service.ts       # Signal-based auth state + localStorage
│   ├── connection.service.ts # Reconnect indicator + probing (expectDisruption / reportUnreachable)
│   ├── form.service.ts       # Abstract FormService<T> — load/save/refresh cycle
│   ├── http.service.ts       # Low-level HttpClient.post wrapper
│   ├── network.service.ts    # Browser online/offline as an observable
│   ├── published-port-deletion.ts # Confirm dialog: change would delete published ports
│   ├── rpc.service.ts        # JSON-RPC 2.0 over HTTP (auto-logout on code 34)
│   ├── system.service.ts     # System info, version, update check
│   ├── vpn-exposed-port.ts   # Confirm dialog: published port bypasses VPN routing
│   └── api/
│       ├── api.service.ts    # Abstract API contract (all RPC methods)
│       ├── live-api.service.ts
│       ├── mock-api.service.ts
│       └── types.ts          # UCI section types + API response types
│
├── utils/
│   ├── validators.ts         # Custom validators: ipv4, ipv6, prefix, mac, hostname, etc.
│   ├── languages.ts          # Supported locales
│   ├── timezones.ts          # Timezone list
│   ├── schedule.ts           # Schedule window utilities
│   ├── masks.ts              # Input masks for forms
│   └── workspace-config.ts   # Type for config.json
│
├── login.ts                  # Login page
│
└── routes/
    ├── setup/                # Initial password setup
    ├── setup-wizard/         # First-time device setup (flash from microSD)
    ├── wan/                  # WAN config (ipv4, ipv6, mac, dns, ddns)
    ├── published-ports/      # Port forwarding — BEST example of table + dialog pattern
    ├── outbound/             # Outbound VPN (WireGuard clients)
    ├── inbound/              # Inbound VPN (WireGuard server + peers)
    ├── ethernet/             # Ethernet port config
    ├── wifi/                 # WiFi (passwords, blackout schedule, settings)
    ├── lan/                  # LAN settings (ipv4, ipv6)
    ├── devices/              # Device management
    ├── profiles/             # Security profiles
    └── settings/             # General, advanced, password, SSH keys, logs, activity, backup
```

The `markdown` pipe and `pauseFor` come from `@start9labs/shared` — they are not local files.

## Routing

Four auth-based guards determine which view loads:

1. **Setup wizard** — `AuthService.setupMode()` is true (booted from removable media)
2. **Initial password** — `AuthService.initialized()` is false (no admin password set)
3. **Authenticated** — `AuthService.authenticated()` is true → full app with nav/aside
4. **Login** — fallback when not authenticated

Authenticated routes are children of the `App` shell component (CSS Grid with header, nav, main, aside), itself mounted inside the router. All routes are lazy-loaded via `loadComponent` / `loadChildren`.

## Route Anatomy

Each route folder typically contains:

| File         | Purpose                                      |
| ------------ | -------------------------------------------- |
| `index.ts`   | Page component (default export, lazy-loaded) |
| `service.ts` | Extends `FormService<T>` — load/save data    |
| `table.ts`   | Table component (for list views)             |
| `dialog.ts`  | Dialog component for add/edit flows          |
| `types.ts`   | Local types and interfaces                   |

Multi-level routes (wan, wifi, outbound, inbound, lan, devices, profiles, settings) use `loadChildren` with child-route folders:

```typescript
export default [
  {
    path: '',
    component: TabsComponent,
    children: [
      { path: 'ipv4', loadComponent: () => import('./routes/ipv4') },
      { path: 'ipv6', loadComponent: () => import('./routes/ipv6') },
      { path: '**', redirectTo: 'ipv4' },
    ],
  },
] satisfies Routes
```

## Project state and forms

`FormService<T>` owns each form's load/save/refresh cycle, polls every five seconds,
and exposes `data` as a signal (`undefined` while loading). `provideFormService`
binds the concrete service and the abstract token at the page component.
The `[formLoading]` directive composes `TuiForm`, `TuiCardLarge`, and `TuiSkeleton`;
`appFooter` supplies the reset/save footer.

`ActionService` owns loading/success/error notifications. Network-affecting actions
use `restart: true` to suppress transient failures and probe until the device is
reachable again. These local services intentionally retain the reconnect behavior
specified in `AGENTS.md`.

Component, form, dialog, and table recipes live in the root `start9-frontend`
skill. `routes/published-ports/` is the project-local example of a table/dialog flow.

## API Layer

- **Transport:** JSON-RPC 2.0 over a single HTTP POST endpoint (`/rpc/v1`)
- **Contract:** `ApiService` (abstract) defines all methods. `LiveApiService` calls `RpcService`; `MockApiService` returns static data for dev.
- **Auth:** RPC error code 34 triggers auto-logout. Session cookie with `withCredentials: true`.
- **Mocks:** Toggle via `config.json` → `useMocks`. Dev server runs without a real router.

## i18n

User-facing strings go through the `i18n` pipe (`{{ 'Save' | i18n }}`), backed by five locale dictionaries (en/es/de/fr/pl) in `src/app/i18n/dictionaries/` — English eager, the rest lazy-loaded. `npm run check:i18n:wrt` (from the repo root) verifies the dictionaries stay in sync. Route-keyed help content in `help/content/` follows the same five-locale pattern.

## Styling Conventions

**No Tailwind.** Uses Taiga CSS variables and global utility classes from `styles.scss`:

| Class                                                                  | Purpose                                            |
| ---------------------------------------------------------------------- | -------------------------------------------------- |
| `g-page`                                                               | Page layout container (flex column, gap)           |
| `g-form`                                                               | Form card (max-width 50rem, field layout rules)    |
| `g-footer`                                                             | Save/Cancel footer row                             |
| `g-table`                                                              | Table wrapper (rounded, neutral bg, sticky header) |
| `g-aside`                                                              | Aside panel content padding                        |
| `g-help`                                                               | Help panel padding rules                           |
| `g-accordion`                                                          | Accordion/details styling                          |
| `g-primary` / `g-secondary` / `g-action` / `g-negative` / `g-positive` | Text color utilities                               |

**Theming:** Dark/light via `tuiTheme` attribute. All colors use `--tui-*` CSS variables.

**Component styles** are inline in `@Component`. Use SCSS. Keep them minimal — lean on Taiga's built-in styling.

## App Configuration

`app.config.ts` provides:

- `provideZonelessChangeDetection()` — no zone.js
- `provideRouter(routes, withPreloading(PreloadAllModules), withRouterConfig({ onSameUrlNavigation: 'reload' }))` — `PreloadAllModules` is deliberate: every lazy chunk is preloaded while the connection is healthy, so an on-demand `import()` never fails against a dead connection during a network restart (which would cache an errored module and leave the route un-loadable until a hard refresh)
- Taiga component option overrides: buttons `m`, badges `m`, tabs `m`, textfields `m`, forms `m`, radios `s`, checkboxes `s`; cards `compact` spacing with the `'floating'` appearance; dialogs size `'s'`
- API service toggle: `MockApiService` or `LiveApiService` based on `config.json`
- HTTP URL: `RELATIVE_URL` token → `/rpc/v1`
