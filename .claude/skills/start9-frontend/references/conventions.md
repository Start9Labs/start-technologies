# i18n, naming & structure

- `| i18n` on every user-facing template string; `inject(i18nPipe).transform(...)` in TS.
  The key **is** the English string; `i18nKey = keyof typeof ENGLISH` makes unknown keys a
  compile error. Backend-localized values (`T.LocaleString`) render via `| localize`.
- Dictionaries: `en.ts` maps English → numeric id; `de/es/fr/pl.ts` map id → translation. Adding
  a string = add to `en.ts` with the next id + real translations in **all** other dictionaries.
  `npm run check:i18n` (and per-app `check:i18n:wrt`/`:tunnel`) scans for misses.
- `shared` hosts the dictionaries for ui/setup-wizard/brochure; start-wrt, start-tunnel, and
  support-server's portal keep local copies of the same machinery (consolidation into shared
  is planned — don't grow them further apart). ops-server and start9-store are English-only:
  hardcode strings there.
- Shared-lib services translate centrally: `DialogService`/`TaskService` accept `i18nKey`-typed
  labels, so callers pass English keys and never pre-translate.

## Naming & structure

- **Files: suffixless, role-named** (new code): a route folder holds `index.ts` (the page,
  `export default class Devices`), `dialog.ts`, `table.ts`, `service.ts`, `types.ts`,
  `utils.ts`, nested `routes/` for tabbed children. Cross-cutting singletons keep
  `.service.ts`/`.pipe.ts`. Class names drop the `Component` suffix (`App`, `Copy`,
  `PublishPortDialog`, `Wifi`). Legacy `.component.ts`/`FooComponent` persists in older trees —
  don't spread it.
- **App skeleton**: `app/{app.ts | main.ts-inline, app.config.ts, app.routes.ts}`,
  `components/` (shared single-file widgets), `services/` (`api/` triad + domain services),
  `pipes/`, `utils/`, `i18n/` (where applicable), `routes/` (one folder per nav item).
- **No barrels in apps** (`index.ts` means "the routed component", not re-exports); libs expose
  one `public-api.ts`. Imports: `src/`-absolute (ui) or relative within a feature; shared code
  by package name (`@start9labs/shared|marketplace|start-core`).
- Import order: external before local, blank line between groups, `@angular/common` before
  `@angular/core`, members alphabetized (normalized by hand in review).
- Umbrella Taiga imports over granular: `TuiDataList, TuiDropdown` — not
  `TuiDataListComponent, TuiDropdownDirective, TuiDropdownOpen, …`.
- Comments explain the non-obvious present — measured values (`// 1120 = 938px of buttons + …`),
  deviations (`// Deliberately NOT tui-root._mobile`), honest placeholders. Attributed TODOs
  mark known debt (`// TODO @Alex refactor this to declarative validation`); divergence from
  canon gets a named owner, not silence.
