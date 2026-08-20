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

<footer appFooter></footer>` — start9-store) or a `TuiNavigation` shell
(`tuiNavigationHeader`/`tuiNavigationAside`/`tuiAsideItem` — start-wrt, start-tunnel).

### The canonical `app.config.ts` (zoneless)

```ts
export const appConfig: ApplicationConfig = {
  providers: [
    provideZonelessChangeDetection(),
    provideBrowserGlobalErrorListeners(),
    provideRouter(routes /*, withInMemoryScrolling(...) etc. as needed */),
    provideHttpClient(withXhr()), // withFetch() where SSR or abort-on-unsubscribe is needed
    provideTaiga({ mode: 'dark' }), // or 'light'; omit mode for runtime TUI_DARK_MODE theming
    // Design-system tuning lives HERE, once, as option providers:
    tuiButtonOptionsProvider({ size: 'm' }),
    tuiTextfieldOptionsProvider({ size: signal('m'), cleaner: signal(false) }),
    tuiCardOptionsProvider({ space: 'compact', appearance: 'floating' }),
    tuiDialogOptionsProvider({ size: 's' }),
    tuiValidationErrorsProvider({
      required: 'This field is required',
      email: 'Please provide valid email address',
      minlength: (e: any) => `Minimum length is ${e.requiredLength} chars`,
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
- `polyfills: []` in `angular.json` is the zoneless tell; zone-based apps list `"zone.js"`.
- Monorepo apps read the gitignored workspace `config.json` **synchronously via `require`**:
  `const { useMocks, gitHash } = require('../../config.json') as WorkspaceConfig` — this feeds
  the Live/Mock DI swap and value tokens (`IS_MOCK`, `GIT_HASH`, `RELATIVE_URL`).
- Router features seen in canon: `withInMemoryScrolling({ scrollPositionRestoration: 'enabled',
anchorScrolling: 'enabled' })` (store), `withComponentInputBinding()` +
  `withPreloading(PreloadAllModules)` + `withDisabledInitialNavigation()` +
  `withRouterConfig({ paramsInheritanceStrategy: 'always' })` (StartOS ui — initial navigation
  deferred until services init inside `provideAppInitializer`). Use what the app needs, nothing
  prophylactically.

### Joining the monorepo vs standing alone

**Monorepo app** (default for products): add a project to the root `angular.json` pointing into
`projects/<product>/web`, wire `package.json` scripts (`start:x`, `build:x`, `check:x`), builder
`@angular/build:application`, `inlineStyleLanguage: "scss"`, budgets with a **per-component
style ceiling** (4kB warn / 8kB error — keeps inline styles honest). Styles array layers:
`@taiga-ui/styles/taiga-ui-theme.less` → (brand apps: `shared-libs/ts-modules/shared/styles/
taiga.scss` + `shared.scss`) → app `styles.scss`. Consume the libs by tsconfig path alias
(`@start9labs/shared` → source, no build step); `@start9labs/start-core` is a built `file:` dep —
run `npm run build:deps` after install or nothing type-checks.

**Standalone repo** (ops tools): copy the start9-store / support-server scaffold — npm
workspace(s), `@angular/build:application`, strict tsconfig +
`strictTemplates`/`strictInjectionParameters`/`strictInputAccessModifiers`, the two
`extendedDiagnostics` suppressions (`nullishCoalescingNotNullable`,
`optionalChainNotNullable` — house policy: harmless-belt-and-suspenders warnings off), Prettier
config as in SKILL.md, husky + lint-staged, `"build": "npm ci && ng build"` for Docker, Express serving
`dist/<app>/browser` with an SPA fallback behind `/_api` routes.

### Icons — three sanctioned setups, one failure mode

Icons are Lucide via `@tui.<name>` (`iconStart="@tui.plus"`, `<tui-icon icon="@tui.check" />`).

| Setup                    | Who                       | How                                                                                                                                                                                                                     |
| ------------------------ | ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Asset copy** (default) | monorepo apps, dashboards | `angular.json` assets: `node_modules/@taiga-ui/icons/src` → `assets/taiga-ui/icons`                                                                                                                                     |
| **postinstall copy**     | start9-store              | hoisted npm workspace can't express the asset path — root `postinstall` runs `fs.cpSync` into `web/public/assets/taiga-ui/icons` (gitignored). Don't "simplify" it back into `angular.json`.                            |
| **Inline registry**      | start-wrt                 | bundle-size-critical (UI embedded in the Rust binary): `angular.json` `loader: { ".svg": "text" }` + `app.icons.ts` importing the used icons from `@taiga-ui/icons/src/*.svg`, registered via `tuiIconsProvider(ICONS)` |

The failure mode: a **hand-curated partial registry** breaks the icons Taiga components draw
_internally_ (select chevrons, dropdown arrows) — exactly such a registry was deleted from
start9-store for this reason. If you inline-register (wrt style), you own keeping the set complete; everywhere
else, serve the whole directory and stop thinking about it. Non-Taiga SVGs ship as plain assets
and are referenced by path, including in icon slots (`iconStart="/x-logo.svg"`).

### Theming and branding

All branding is a **`--tui-*` design-token override sheet**, nothing else:

- Monorepo brand apps: `shared-libs/ts-modules/shared/styles/taiga.scss` — the Start9 dark theme
  as a `[tuiTheme='dark']` block (backgrounds, statuses with `color-mix()` pale variants, text
  tiers), Hanken Grotesk `@font-face` + `--tui-typography-family-*`, a custom
  `[tuiAppearance][data-appearance='primary-success']` built with Taiga's SCSS mixins
  (`@use '@taiga-ui/styles/utils'`), and shrinking upstream shims (pruned as Taiga absorbs fixes).
- start-wrt: dual-theme — tokens under both `[tuiTheme='dark']` and `:root, [tuiTheme='light']`;
  runtime switch via the `TUI_DARK_MODE` signal (`inject(TUI_DARK_MODE).set(...)`), regional via
  `[attr.tuiTheme]`.
- Dashboards: ~15 `:root` token overrides (accent `#07a4ff` family) + Montserrat, full stop.
- start9-store: stock Taiga light + ~10 token overrides; `provideTaiga({ mode: 'light' })`.

Hex colors live **only** in the theme sheet. Components use `var(--tui-...)` exclusively —
`--tui-text-secondary`, `--tui-status-negative`, `--tui-background-neutral-1`,
`--tui-border-normal`, `font: var(--tui-typography-body-l)`, `var(--tui-radius-m)`. A hex
literal in a component is a review comment waiting to happen.

Breakpoints: Taiga's `TUI_BREAKPOINT` signal and `tui-root._mobile` follow `TUI_MEDIA` — which
an app may override when its header demands it (store: `mobile: 1120`, **measured** — if you add
a nav item, re-measure and raise it). Components that shouldn't collapse at the app-wide
threshold use their own `@media` — deliberately, with a comment.
