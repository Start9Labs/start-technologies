# Taiga UI 5 — verified quick reference

Doc-verified against taiga-ui.dev and the published `@taiga-ui/*@5.x` typings. Taiga 5
requires Angular ≥ 19, uses **no `@angular/animations`**, and deleted `@taiga-ui/legacy` —
nothing may import it. When in doubt: the official MCP
(`npx @taiga-ui/mcp@latest --source-url=https://taiga-ui.dev/llms-full.txt`) or
`https://taiga-ui.dev/llms-full.txt`.

### v5 renames & dead APIs (your training data is probably stale)

| If you remember…                                           | v5 reality                                                                                     |
| ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `TuiAlertService` / `[tuiAlert]`                           | `TuiNotificationService` / `[tuiNotification]` (kit also adds a new compact `TuiToastService`) |
| `tui-input`, `tui-input-password`, … wrappers              | gone — native `<input tuiInput>` etc. inside `<tui-textfield>`                                 |
| inner directive `tuiTextfield` on the input                | renamed `tuiInput`                                                                             |
| `<tui-avatar>`, `<tui-badge>`, `<tui-tag>`                 | `[tuiAvatar]`, `<span tuiBadge>`, `<span tuiChip>` attributes                                  |
| Loader `[showLoader]`                                      | `[loading]`                                                                                    |
| `NG_EVENT_PLUGINS`, `provideAnimations()`                  | one `provideTaiga(options?)` (event plugins included)                                          |
| `TuiFieldErrorPipe` (`tuiFieldError                        | async`)                                                                                        | `TuiErrorPipe` (`tuiError`) — house style: bare `<tui-error formControlName>` + provider map |
| `tuiPure`, `TuiLet`, `TuiRepeatTimes`, `TuiDestroyService` | gone — `computed()`/pipes, `@let`, `@for`, `takeUntilDestroyed()`                              |
| `tuiCreateToken` / `tuiCreateTokenFromFactory`             | not in v5 cdk — `new InjectionToken(desc, { factory })` or `tuiCreateOptions`                  |
| `TUI_IS_MOBILE`                                            | `WA_IS_MOBILE` (`@ng-web-apis/platform`; also `WA_IS_IOS/ANDROID`, `WA_REDUCED_MOTION`)        |
| `TuiCell` in layout; Checkbox/Radio/Slider in kit          | moved to `@taiga-ui/core`                                                                      |

### The cdk toolbox (`@taiga-ui/cdk`)

- **DI**: `tuiProvide(TOKEN, UseExisting)`; `tuiProvideOptions(token, partial, fallback)`;
  `tuiCreateOptions(defaults)` → `[TOKEN, tuiXOptionsProvider]` pair (build your own
  configurable components exactly like Taiga's); `tuiFallbackValueProvider`;
  `tuiDirectiveBinding` (drive a host directive's input from a wrapper).
- **Custom form controls**: extend **`TuiControl<T>`** + provide with `tuiAsControl(MyControl)`
  — signal-based CVA base (`value()`, `disabled()`, `invalid()`, `readOnly` input) that
  replaces 40 lines of boilerplate; `TuiValueTransformer` translates stored ↔ view values.
- **Directives**: `TuiActiveZone` + `TuiObscured` (the correct "clicked outside / focus left"
  primitives — never `document.addEventListener`), `TuiAutoFocus`, `TuiValueChanges`,
  `TuiValidator`, `TuiNativeValidator`, `TuiItem`, `TuiHovered`, `TuiFocusTrap`, `TuiPan`/
  `TuiSwipe`/`TuiZoom`, `TuiCopyProcessor`, `TuiMedia`, `TuiAnimated`, `TuiWithStyles`.
- **Functions/observables**: `tuiControlValue(control)` (valueChanges incl. current value),
  `tuiTakeUntilDestroyed`, `tuiTypedFromEvent`, `tuiMarkControlAsTouchedAndValidate`,
  `tuiInjectElement()`, `tuiWindowSize`, `tuiIsPresent`/`tuiIsString`, `tuiSetSignal`,
  `TUI_TRUE_HANDLER`/`TUI_FALSE_HANDLER`; `TuiMapperPipe` (`value | tuiMapper: fn : args` —
  pure template mapping without component methods), `TuiFilterPipe`, `TuiObfuscatePipe`.
- Types: `TuiBooleanHandler`, `TuiStringHandler`, `TuiContext<T>`, `TuiStringMatcher`,
  `TuiIdentityMatcher`, `TuiValidationError`.

### Appearances & theming

- `[tuiAppearance]` writes `data-appearance`; extra inputs `tuiAppearanceState` (force
  hover/active), `tuiAppearanceFocus`, `tuiAppearanceMode`. Bundled names: `primary`,
  `secondary`, `flat`, `outline`, `action`, `accent`, `icon`, `floating`, `textfield`,
  `neutral`/`positive`/`negative`/`warning`/`info` (+ `-grayscale` variants used fleet-wide).
- **Custom appearance = CSS, not TS**, in the theme sheet with the state mixins:

```scss
@use '@taiga-ui/styles/utils';
[tuiAppearance][data-appearance='primary-success'] {
  background: var(--tui-status-positive);
  .appearance-hover({ … }); .appearance-active({ … }); .appearance-disabled({ … });
}
```

- **Tokens** (override at any DOM level; this _is_ branding): backgrounds
  (`--tui-background-base|-neutral-1…|-accent-1…|-elevation-1/2/3`), text (`--tui-text-primary|
-secondary|-tertiary|-action|-primary-on-accent-1`), status (`--tui-status-negative|-positive|
-warning|-info` + `-pale` variants), borders (`--tui-border-normal|-hover|-focus`), shadows
  (`--tui-shadow-small|-medium|-popup`), sizing (`--tui-height-l/m/s/xs`, `--tui-padding-*`,
  `--tui-radius-l/m/s/xs`), typography composites (`font: var(--tui-typography-body-s)`,
  families `--tui-typography-family-display/-text`), charts `--tui-chart-categorical-00…22`.
- **Dark mode**: `TUI_DARK_MODE` — injectable `WritableSignal<boolean>` with `.reset()`;
  persists to `localStorage['tuiDark']`, seeds from `prefers-color-scheme`. Scope a subtree
  with the `[tuiTheme]` attribute.
- **Breakpoints**: `TUI_BREAKPOINT` signal emits `'mobile' | 'desktopSmall' | 'desktopLarge'`
  (thresholds from `TUI_MEDIA`, overridable); CSS mixins `@tui-mobile`/`@tui-tablet`/
  `@tui-desktop` in `@taiga-ui/styles/utils` (boundaries ≈ 767.4 / 1023.4 / 1279.4 px).
- `[tuiSkeleton]="loadingOrLineCount"` on any element; `tuiFade` / `tui-line-clamp` for
  truncation; `<tui-scrollbar>` for themed scrollbars (`provideTaiga({ scrollbars: 'native' })`
  opts out — the fleet's embedded UIs do). `tui-scrollbar`'s inner `.t-content` is
  `min-inline-size: fit-content`, so one `white-space: nowrap` descendant widens the whole
  scroll area and pushes a row's trailing content out of view — pair `tuiFade` truncation
  inside a scrollbar with `:host ::ng-deep tui-scrollbar > .t-content { min-inline-size: 0 }`
  (`.t-content` alone ties on specificity and loses).

### Overlay facts

- `tuiDialog(Component, options)` factory (typed data/result inference) is valid v5 API — the
  fleet standardizes on `TuiResponsiveDialogService`/shared `DialogService` instead; don't mix
  per app. `TuiDialogOptions`: `label`, `size: 's'|'m'|'l'`, `data`, `closable`, `dismissible`,
  `required`, `appearance` (`'fullscreen'` for mobile-style takeover).
- Declarative twins exist for all three: `[(tuiDialog)]`, `[(tuiNotification)]`, `[(tuiToast)]`
  on `ng-template`, each with an `[…Options]` input — pair with a `signal(false)`.
- `TuiConfirmService.withConfirm(options)` + `markAsDirty()/markAsPristine()` is the
  unsaved-changes guard behind `FormDialogService`.
- Dropdown flavors: `tuiDropdownHover`, `tuiDropdownContext` (right-click), `tuiDropdownManual`,
  `[(tuiDropdownOpen)]`, `tuiDropdownAuto`; nested menus via kit's `TuiDataListDropdownManager`.
- Hints: `tuiHintDescribe` (a11y, keyboard-triggered), `tuiHintManual`, `tuiHintPointer`,
  `tuiHintOverflow` (only when truncated); `tui-icon[tuiTooltip]` for the help-icon shorthand.

### Textfield facts

`<tui-textfield>` (and `tui-textfield[multi]` for chips) wraps a **native** element:
`input[tuiInput]`, `input[tuiSelect]`, `input[tuiComboBox]` (`[strict]`, `[matcher]`),
`input[tuiInputNumber]`, `textarea`, date/time/phone/pin variants — all extending `TuiControl`.
Sizing via `[tuiTextfieldSize]` / `tuiTextfieldOptionsProvider`; `[content]` (Polymorpheus)
renders the selected value, `[filler]` ghost-texts. Dropdown content attaches with
`*tuiDropdown` **inside the textfield**; `tui-data-list-wrapper` takes `[items]` (`null` =
loading state), `[itemContent]`, `[emptyContent]`; filter with `| tuiFilterByInput`;
`[stringify]`/`[identityMatcher]` come from `TuiItemsHandlers`. `tuiChevron` adds the arrow.

### Polymorpheus (`@taiga-ui/polymorpheus`)

The mechanism behind every `content`/`label`/`itemContent` API. `PolymorpheusContent<C>` is the
input type (accepts string | function of context | template | component);
`*polymorpheusOutlet="content as text; context: ctx"` renders it; `PolymorpheusComponent`
wraps a component (+optional injector); `injectContext<T>()` reads the live context inside a
dynamically created component. Declare `PolymorpheusContent` inputs on your own components
instead of forking them per content variant.

### Event plugins (`@taiga-ui/event-plugins`, auto-installed by `provideTaiga`)

Modifiers: `.prevent`, `.stop`, `.self`, `.capture`, `.once`, `.passive`, **`.zoneless`** (skip
change detection — high-frequency events), **`.debounce~300ms`**, **`.throttle~500ms`**.
Global targets in templates: `(window>resize)`, `(document.click)`, `(visualViewport>resize)`.

### Maskito (`@maskito/*`)

`[maskito]="options"` on the native input (`MaskitoDirective` from `@maskito/angular`);
`MaskitoOptions = { mask: RegExp | (string|RegExp)[] | fn, plugins, pre/postprocessors }`;
kit generators: `maskitoNumberOptionsGenerator`, `maskitoDateOptionsGenerator`,
`maskitoTimeOptionsGenerator`, `maskitoDateRangeOptionsGenerator` (+ parse/stringify twins,
`maskitoWithPlaceholder`, caret/focus plugins); `maskitoTransform` for static formatting.
Handles paste/drop/autofill/predictive keyboards; SSR-safe. Never keydown-regex filtering.

### `@ng-web-apis/*` tokens

`common`: `WA_WINDOW`, `WA_NAVIGATOR`, `WA_LOCATION`, `WA_HISTORY`, `WA_LOCAL_STORAGE`,
`WA_SESSION_STORAGE`, `WA_USER_AGENT`, `WA_ANIMATION_FRAME`, `WA_PAGE_VISIBILITY`,
`WA_PERFORMANCE`, `WA_CRYPTO`, `WA_MEDIA_DEVICES`, `WA_SCREEN`. `platform`: `WA_IS_MOBILE`,
`WA_IS_IOS`, `WA_IS_ANDROID`, `WA_IS_WEBKIT`, `WA_REDUCED_MOTION`. `universal`:
`provideUniversal()` — server-safe mocks for SSR (what start9-store uses; never hand-roll
`matchMedia` stubs). Plus observer packages (`intersection-observer`, `resize-observer`,
`mutation-observer` — `WaIntersectionObserver`, `WaMutationObserver` directives as used in
`LogsWindowComponent`), `storage`, `geolocation`, `speech`, `view-transition`, `workers`.

### Need → wrong instinct → right primitive

| Need                                                             | Agent instinct                          | House answer                                                                                                    |
| ---------------------------------------------------------------- | --------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Close on click-outside/Esc                                       | `document.addEventListener`             | `[tuiDropdown]`, or `TuiActiveZone` + `TuiObscured`                                                             |
| Tooltip                                                          | CSS `::after` bubble                    | `[tuiHint]` / `tui-icon[tuiTooltip]`                                                                            |
| Toast                                                            | fixed-position div + timeout            | `TuiNotificationService` (queued, positioned)                                                                   |
| Blocking "working…"                                              | full-screen spinner div                 | `TuiNotificationMiddleService` (hold subscription, `unsubscribe()` to close)                                    |
| Modal with typed result                                          | `@if` overlay / CDK overlay             | `TuiResponsiveDialogService` + Polymorpheus + `injectContext`                                                   |
| Confirm prompt                                                   | custom dialog component                 | `TUI_CONFIRM` + `TuiConfirmData`                                                                                |
| Input mask                                                       | keydown regex                           | Maskito                                                                                                         |
| Custom form control                                              | 40-line CVA                             | `TuiControl<T>` + `tuiAsControl`                                                                                |
| Validation messages                                              | `@if (control.errors?.required)` chains | `<tui-error formControlName>` + `tuiValidationErrorsProvider`                                                   |
| Dark mode                                                        | theme service + class toggle            | `TUI_DARK_MODE` signal (+ `[tuiTheme]` scoping)                                                                 |
| Colors/spacing                                                   | hex + magic paddings                    | `--tui-*` tokens                                                                                                |
| Responsive TS logic                                              | `window.innerWidth`                     | `TUI_BREAKPOINT` / `WA_IS_MOBILE`                                                                               |
| Responsive CSS                                                   | `@media (max-width: 768px)`             | `tui-root._mobile &` / styles-utils mixins                                                                      |
| Skeleton                                                         | hand-rolled shimmer CSS                 | `[tuiSkeleton]`                                                                                                 |
| Truncation                                                       | `text-overflow` fights                  | `tuiFade` / `tui-line-clamp`                                                                                    |
| Page scaffolding                                                 | bespoke flex/grid                       | `tuiCardLarge`+`tuiSurface`, `tuiHeader`+`tuiTitle`, `tuiCell`, `tuiForm`, `tuiNavigation*`, `tui-block-status` |
| Select/autocomplete                                              | hand-built dropdown                     | `tui-textfield` + `tuiSelect`/`tuiComboBox` + `*tuiDropdown` + `tui-data-list-wrapper` + `tuiFilterByInput`     |
| Browser globals                                                  | `window.` / `localStorage.`             | `WA_*` tokens                                                                                                   |
| High-frequency events                                            | melting change detection                | `.zoneless` / `.debounce~` / `.throttle~` modifiers                                                             |
| `preventDefault`                                                 | `$event.preventDefault()` in TS         | `(event.prevent)` modifier                                                                                      |
| Configurable content slot                                        | fork the component                      | `PolymorpheusContent` input                                                                                     |
| Copy button, rating, pagination, PIN, carousel, avatar-initials… | write it                                | it's in kit — check the docs first                                                                              |
| Empty state                                                      | custom illustration div                 | `tui-block-status`                                                                                              |
