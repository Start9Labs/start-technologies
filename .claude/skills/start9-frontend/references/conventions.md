# i18n, naming & structure

`FRONTEND.md` says whether the app translates, where its dictionaries live and which command
checks them. An app without i18n hardcodes English strings.

- `| i18n` on every user-facing template string; `inject(i18nPipe).transform(...)` in TS.
  The key **is** the English string; `i18nKey = keyof typeof ENGLISH` makes unknown keys a
  compile error. Backend-localized values (a `LocaleString`) render via `| localize`.
- Dictionaries: `en.ts` maps English → numeric id; each other dictionary maps id →
  translation. Adding a string = add to `en.ts` with the next id + real translations in **all**
  other dictionaries, then run the app's i18n check.
- An app that keeps local dictionaries uses the same machinery as the shared ones; don't grow
  the copies further apart.
- Shared services translate centrally: a dialog or action wrapper accepts `i18nKey`-typed
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
  one `public-api.ts`. Imports: `src/`-absolute or relative within a feature; shared code by
  package name, never a relative path into the library.
- Import order: external before local, blank line between groups, modules sorted by path
  within a group (`@angular/common`, `@angular/core`, `@taiga-ui/addon-mobile`,
  `@taiga-ui/cdk`, `@taiga-ui/core`, `rxjs`), members alphabetized (normalized by hand in
  review).
- Umbrella Taiga imports over granular: `TuiDataList, TuiDropdown` — not
  `TuiDataListComponent, TuiDropdownDirective, TuiDropdownOpen, …`.
- Comments follow the repo's comment rules (its root `AGENTS.md`): none
  unless a fact can't be recovered from the code, such as a measured value
  (`// 1120 = 938px of buttons + …`) or a deliberate divergence
  (`// Deliberately NOT tui-root._mobile`). Known debt goes in an issue, not a TODO.
