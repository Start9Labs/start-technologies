# Bootstrapping a Start9 UI (app.config, workspace, icons, theming)

### `main.ts` and the root component

`main.ts` is ~5 lines: `bootstrapApplication(App, appConfig).catch(err => console.error(err))`.
The root template is nothing but:

```ts
@Component({
  selector: 'app-root',
  imports: [RouterOutlet, TuiRoot],
  template: '<tui-root><router-outlet /></tui-root>',
  styles: `
    :host {
      height: 100%;
      display: block;
    }
    tui-root {
      height: 100%;
    }
  `,
})
export class App {}
```

`TuiRoot` hosts all portals (dialogs, alerts, dropdowns, hints) — never add outlets for them.
Shell chrome (nav, header, footer) is either inside the root template as **attribute-selector
components on semantic elements** (`<header appHeader></header> <main><router-outlet /></main>

<footer appFooter></footer>`, a public site) or a `TuiNavigation` shell
(`tuiNavigationHeader`/`tuiNavigationAside`/`tuiAsideItem`, an admin UI).

In a `TuiNavigation` shell the scroll area must end exactly at `main[tuiNavigationMain]`'s
content box: its `tui-scrollbar` is `100%` tall less its own block padding
(`block-size: calc(100% - 1.5rem)` beside `padding-block-start: 1.5rem`). Any taller and the
section scrolls instead, dropping the sticky aside below the header and its footer off-screen.
On mobile, move `tui-scroll-controls` into main's thin border with a `translateX` rather than
widening the margin, or the bar covers the cards. A dual-theme app sets
`--tui-theme-color` on the header and aside themselves, not on the shell: Taiga renders them as
dark subtrees in the light theme too, so a value set higher up resolves the light token and
leaves white chrome under white text.

### The canonical `app.config.ts` (zoneless)

```ts
export const appConfig: ApplicationConfig = {
  providers: [
    provideZonelessChangeDetection(),
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes, withComponentInputBinding() /*, withInMemoryScrolling(...) as needed */),
    provideHttpClient(withXhr()), // withFetch() where SSR or abort-on-unsubscribe is needed
    provideTaiga({ mode: 'dark' }), // or 'light'; omit mode for runtime TUI_DARK_MODE theming
    // Design-system tuning lives HERE, once, as option providers:
    tuiButtonOptionsProvider({ size: 'm' }),
    tuiTextfieldOptionsProvider({ size: signal('m') }),
    tuiCardOptionsProvider({ space: 'compact', appearance: 'floating' }),
    tuiDialogOptionsProvider({ size: 's' }),
    tuiValidationErrorsProvider({
      required: 'This field is required',
      email: 'Please provide valid email address',
      minlength: ({ requiredLength }: { requiredLength: number }) => `Minimum length is ${requiredLength} chars`,
    }),
    // App wiring:
    provideAppInitializer(() => inject(AuthService).whenReady()),
    { provide: ApiService, useClass: useMocks ? MockApiService : LiveApiService },
    tuiProvide(AbstractSomething, ConcreteSomething), // every useExisting binding
  ],
}
```

Notes, all load-bearing:

- `provideTaiga(...)` replaces `provideAnimations()` + `NG_EVENT_PLUGINS` + `tuiTheme`
  attributes + manual `TUI_VALIDATION_ERRORS`-era wiring. Theme is DI (`mode`), not markup.
- Option providers with **signals as option values** are normal
  (`tuiTextfieldOptionsProvider({ size: signal('m') })`).
- A zone-based app lists `"zone.js"` in its `angular.json` `polyfills`; a zoneless one doesn't.
- Build-time config (the mock switch, the git hash) is read **synchronously** at module load,
  never fetched: a gitignored `config.json` through `require`, or `environment.ts`. It feeds the
  Live/Mock DI swap and value tokens (`IS_MOCK`, `GIT_HASH`, `RELATIVE_URL`). `FRONTEND.md`
  says which file.
- Router features seen in canon: `withInMemoryScrolling({ scrollPositionRestoration: 'enabled',
anchorScrolling: 'enabled' })` (a public site), `withComponentInputBinding()` +
  `withPreloading(PreloadAllModules)` + `withDisabledInitialNavigation()` +
  `withRouterConfig({ paramsInheritanceStrategy: 'always' })` (an app whose initial navigation
  waits for services to initialize inside `provideAppInitializer`). Use what the app needs,
  nothing prophylactically.

### Joining a multi-app workspace vs standing alone

**Workspace app**: add a project to the workspace's `angular.json`, wire `package.json`
scripts (`start:x`, `build:x`, `check:x`), builder `@angular/build:application`,
`inlineStyleLanguage: "scss"`, budgets with a **per-component style ceiling** (4kB warn / 8kB
error — keeps inline styles honest). Styles array layers: `@taiga-ui/styles/taiga-ui-theme.less`
→ the shared brand sheets, where the app uses them → app `styles.scss`. Consume shared libraries
by tsconfig path alias to their source, with no build step. The repo-root `FRONTEND.md` names
the sheets, the libraries and any prebuilt dependency.

**Standalone repo**: copy the newest standalone sibling's scaffold — npm
workspace(s), `@angular/build:application`, strict tsconfig +
`strictTemplates`/`strictInjectionParameters`/`strictInputAccessModifiers`, the two
`extendedDiagnostics` suppressions (`nullishCoalescingNotNullable`,
`optionalChainNotNullable` — house policy: harmless-belt-and-suspenders warnings off), Prettier
config as in SKILL.md, husky + lint-staged, `"build": "npm ci && ng build"` for Docker, Express serving
`dist/<app>/browser` with an SPA fallback behind `/_api` routes.

### Icons — three sanctioned setups, one failure mode

Icons are Lucide via `@tui.<name>` (`iconStart="@tui.plus"`, `<tui-icon icon="@tui.check" />`).

| Setup                    | When                                                      | How                                                                                                                                                              |
| ------------------------ | --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Asset copy** (default) | `angular.json` can reach the icons package                | `angular.json` assets: `node_modules/@taiga-ui/icons/src` → `assets/taiga-ui/icons`                                                                              |
| **postinstall copy**     | a hoisted npm workspace that can't express the asset path | root `postinstall` runs `fs.cpSync` into `web/public/assets/taiga-ui/icons` (gitignored). Don't "simplify" it back into `angular.json`.                          |
| **Inline registry**      | bundle-size-critical, such as a UI embedded in a binary   | `angular.json` `loader: { ".svg": "text" }` + `app.icons.ts` importing the used icons from `@taiga-ui/icons/src/*.svg`, registered via `tuiIconsProvider(ICONS)` |

The failure mode: a **hand-curated partial registry** breaks the icons Taiga components draw
_internally_ (select chevrons, dropdown arrows). If you inline-register, you own keeping the set
complete; everywhere else, serve the whole directory. `FRONTEND.md` records a non-default setup. Non-Taiga SVGs ship as plain assets
and are referenced by path, including in icon slots (`iconStart="/x-logo.svg"`) — that renders
the file as a `currentColor` mask, so it suits a monochrome logo only. A full-colour raster or
SVG (a package icon from `icon_url`) goes through the same slot as `iconStart="@img.<url>"`,
which draws it as a background at the slot's icon size instead of masking it.

### Theming and branding

All branding is a **`--tui-*` design-token override sheet**, nothing else. `FRONTEND.md` says
where the app's sheet lives and which theme it runs.

- A sheet holds token overrides (backgrounds, statuses with `color-mix()` pale variants, text
  tiers), the brand font's `@font-face` + `--tui-typography-family-*`, custom appearances built
  with Taiga's SCSS mixins (`@use '@taiga-ui/styles/utils'`), and shrinking upstream shims,
  pruned as Taiga absorbs fixes. A small app's sheet is a dozen or so `:root` overrides and a
  font, full stop.
- Single-theme: `provideTaiga({ mode: 'dark' | 'light' })` and the overrides under that theme.
- Dual-theme: tokens under both `[tuiTheme='dark']` and `:root, [tuiTheme='light']`; runtime
  switch via the `TUI_DARK_MODE` signal (`inject(TUI_DARK_MODE).set(...)`), regional via
  `[attr.tuiTheme]`.

Hex colors live **only** in the theme sheet. Components use `var(--tui-...)` exclusively —
`--tui-text-secondary`, `--tui-status-negative`, `--tui-background-neutral-1`,
`--tui-border-normal`, `var(--tui-radius-m)`. A hex literal in a component is a review
comment waiting to happen.

A piece that sits on a dark or light ground — a logo tile, a card on a page — takes
`tuiTheme` (`[attr.tuiTheme]="mark.ground"`, `<business-hire tuiTheme="dark">`) and Taiga's own
tokens (`background: var(--tui-background-base-alt)`), not a `data-ground` attribute mapped to
brand colours. Taiga's palette ships `[tuiTheme='dark']` whatever `provideTaiga`'s `mode`, so
this works in light-only apps too.

Each theme defines its own status text colours. In a dual-theme sheet, the light block must not
reuse the dark block's `--tui-text-negative`, `-positive` or `-warning` values. Those colours
are tuned for dark backgrounds and are too faint on white (`#ff4961` ≈3.3:1, `#2dd55b`
≈1.9:1). Either leave the light block on Taiga's light palette (`#eb1700`, `#008764`) or
override it with a colour that reaches 4.5:1 against `--tui-background-base`. Error text is body
text, so it needs body-text contrast.
