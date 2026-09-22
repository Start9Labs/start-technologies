# Antipattern catalog — what gets rewritten in review

Extracted from the refactor diffs that shaped the fleet, ranked by how often the same fix
recurs. The canonical cleanup refactors are **net-negative in 9 of the 10 largest cases**. The
recurring diagnosis: _someone hand-built a thing the platform already provides — a card, a
loader/try/catch, a mobile layout, a form class, a color, a wrapper element — and the fix is to
find the primitive, configure it once via DI, and delete the rest._

**1. Repeated styling attributes / CSS overrides of Taiga → option provider.**

```html
<!-- BEFORE: appearance="flat" repeated on 8 nav buttons; or CSS:
     a[tuiLink] { color: var(--tui-text-secondary); font: var(--tui-typography-body-s) } -->
```

```ts
// AFTER: one line, zero CSS
providers: [tuiButtonOptionsProvider({ appearance: 'flat-grayscale', size: 'm' })]
providers: [tuiLinkOptionsProvider({ appearance: 'action-grayscale' })]
```

**2. Hand-rolled card/heading/list CSS → layout primitives.** Custom `.card`/`.head` divs with
60 lines of CSS become `<section tuiCardLarge="compact" appearance="secondary-grayscale">` +
`<header tuiHeader><h3 tuiTitle>` + `<ul tuiList="s">`. A `role="table"` div grid with 60 lines
of CSS becomes `tuiCell` rows + `| keyvalue`. If you're writing flexbox to lay out a
title/subtitle/actions row, you missed a primitive.

**3. Per-component loader + try/catch + error toast → `TaskService.run(task, 'Saving')`.**
(One sweep collapsed ~50 copies into one 30-line service.) Caveat from the follow-up commit:
where a flow needs raw promise semantics, that call site went back to a manual loader — the
abstraction is not forced where it doesn't fit.

**4. Duplicated desktop/mobile DOM → one DOM restyled per breakpoint.**
`<div class="desktop"><table>…</table></div><div class="mobile">…re-marked-up…</div>` becomes a
single table whose cells self-label (`<td [attr.data-label]="'Type' | i18n">`) restyled under
`:host-context(tui-root._mobile)` (−90 lines per component). Use the `TUI_BREAKPOINT` signal +
`@if` only when the branches genuinely differ.

**5. Wrapper elements → attribute components on semantic hosts.** `selector: 'app-footer'`
wrapping `<footer>` becomes `selector: 'footer[appFooter]'`; a `.shell` wrapper div is deleted
in favor of semantic elements directly inside `tui-root`; an element component wrapping
`<a tuiButton>` becomes `selector: 'a[marketplacePackageLink]'` with `hostDirectives:
[TuiButton]`, static `host` attrs and option providers; an avatar component that was
`:host { display: contents }` around `<span tuiAvatar [size]="size()">` becomes
`hostDirectives: [{ directive: TuiAvatar, inputs: ['size'] }]` with its own `size` input
deleted.

**6. Semantic-HTML repair.** `<h3>` abused for body text (with CSS undoing it) → plain text +
`font: var(--tui-typography-body-l)`; `<h2>` for dialog sections → `<h3 tuiHeader="h6">`
(heading _level_ decoupled from visual _size_); `<h1>`+`<p>` → `<hgroup tuiTitle>` +
`tuiSubtitle`; click-handler navigation → real `<a tuiButton [href]>`; icon-only buttons get
text content (Taiga hides it visually) instead of `aria-label`; bare `<label>` + flex CSS →
`<label tuiLabel>`; styled `<span class="divider">` → `<hr>`; `autocomplete="new-password"` on
password fields; `TuiAutoFocus` on the first focusable in dialogs/drawers.

**7. Fine-tuning CSS → deleted.** Every `letter-spacing`, `text-transform`, `vertical-align`
nudge, and `margin-top` ladder goes; vertical rhythm comes from `:host { display: grid;
gap: 2rem }`. Physical properties → logical (`inset-block-start`, `inline-size`,
`margin-block-end`) in every touched file.

**8. Specificity hacks → deleted.** `!important` globals get scoped without it; `::ng-deep`
layout pokes die with the primitive that replaces them; custom appearance CSS
(`[tuiCardLarge] { background: … }`) → `appearance="floating"`. When a global override _is_
warranted: the doubled-selector trick (`.g-negative.g-negative { … }`,
`tui-dropdown[data-appearance='start-os'][data-appearance='start-os']`), never `!important`.

**9. Form ceremony → `NonNullableFormBuilder` shorthand** (see forms.md), with single-use private
fields inlined into the group literal.

**10. Single-use names, wrapper methods, multi-branch returns → inlined expressions.**
`select(type) { this.context.completeWith(type) }` + `(click)="select('public')"` →
`(click)="context.completeWith('public')"`; `sentTo = this.verification.sentTo` plus
`resend() { return this.verification.resend() }` → `protected readonly verification` and
`verification.sentTo()` / `(click)="verification.resend()"` in the template.
`if`-ladders → one boolean expression or ternary;
`p.length > 0` → `!!p.length`; flag parameters → default parameters; `try/catch` around a
subscribe callback → deleted.

**11. Derived-state ceremony → template pipes.** A `computed()` mapping
`Object.entries(x).map(([name, value]) => ({name, value}))` → `@for (row of x | keyvalue)`;
a `switchMap(async …)` that builds an object → `map(c => c ?? defaultConfig)`;
`takeUntilDestroyed(this.destroyRef)` + stored `DestroyRef` → bare `takeUntilDestroyed()`.

**12. `track $index` on entity lists → `track item` / `track item.id`** ("service table DOM
cached"). `$index` is for genuinely static lists.

**13. Manual reset plumbing → `linkedSignal`.** `menuOpen = signal(false)` + `.set(false)`
sprinkled across handlers → one `linkedSignal({ source, computation: () => false })` keyed on
navigation/breakpoint. Conversely, needlessly added signal ceremony gets deleted (a
`TitleStrategy` injection where `toSignal(router.events)` as a change trigger sufficed).

**14. DI style.** `private readonly patch: PatchDB<DataModel> = inject(PatchDB)` →
`inject<PatchDB<DataModel>>(PatchDB)`; `{ provide, useExisting }` → `tuiProvide`; inject the
**narrow** dependency (`PATCH_CACHE` observable, not the whole `PatchDB`) to break cycles; type
against the abstract class — "the less concrete you define it here the better"; a child may
inject its parent component instance (`inject(MarketplacePreviewComponent)`) instead of
prop-drilling; hand-rolled unions → library types (`PolymorpheusContent`).

**15. Dead code dies immediately.** Unused components, `.html`/`.scss` orphans,
`.asObservable()` no-ops, post-upgrade `// TODO: Remove in Taiga v5.0` shims (actually removed
after the upgrade), unused imports, five font families the day the design settled on one.

**16. `input<T | null>(null)` → `input<T>()`** and make consumers tolerate `undefined` —
"it's better to make the argument optional so `undefined` will not cause type error." Delete
the `@if` guard around `*ngTemplateOutlet` (it no-ops on nullish).

**17. Route redirects: `{ path: '**', redirectTo: … }`last**, not`{ path: '',
redirectTo, pathMatch: 'full' }` first.

**18. Utility-class hygiene.** `g-*` utilities exist once in the shared/global sheet — per-app
copies get centralized, one-off spacing classes (`.padding-top`) get deleted. The global
stylesheet count goes down, never up.

**19. Copy-pasted markup branches → data-driven rendering.** Three hand-written dropdown
templates + eight nav buttons → one `navigation` object rendered via `| keyvalue: asIs` (with
`asIs = () => 0` preserving insertion order); string-vs-object value discriminates leaf vs
dropdown; `/`-prefix discriminates routerLink vs external href.

**20. Copy is sentence case.** `'Beginning Backup'` → `'Beginning backup'`. Title Case only
for proper nouns and page titles.

**21. Hand-built kit pieces → the kit.** An `initial(name)` util → `| tuiInitials`; three
`label tuiBlock` + `input tuiRadio` rows in a `tuiGroup` → one `tui-radio-list`; a
`[tuiDropdown]="tpl"` menu with an `open` signal reset in every handler → `tuiDropdownAuto` +
`*tuiDropdown="let close"`; `section > [tuiCell] { inline-size: 100% }` → `tuiCellStretch`.

### Review quotes (verbatim, from actual PRs)

- On template function calls: _"This way you call this function on each change detection,
  effectively creating a new Observable each time. You need a readonly property… or a custom
  pipe so that it is only called once."_
- On submit buttons: _"There's no way to click save until the form is valid because it's
  disabled. A good UX pattern is to not disable it… so that I can see all the fields I forgot
  to type in."_ → enabled submit + `tuiMarkControlAsTouchedAndValidate(this.form)` on click.
- On dialog copy: _"It makes more sense for longer text to be in `data: { content }` rather
  than in the title… it looks better with a title and an actual message than with just a huge
  title."_
- On error paths: _"Do we want to complete it on finally and not in successful try? What if we
  lose connection while entering the values — we would then lose the form. Is that ok?"_
- On component size: _"This component gets rather huge. I would move backing-up mode to a
  separate component."_ / _"It's easier to maintain when there's a single component per file"_
  (tiny private helpers — a toast, a dialog's `PolymorpheusComponent` const — may co-locate).
- On globals: _"It's good practice not to access global objects like that"_ — DI tokens.
- On RxJS: observables end in `$`; `defer(() => this.api.call().pipe(catchError(…)))` over
  fetch-in-`ngOnInit`; drop `async`/`switchMap` when nothing is async — use `map`; no `else`
  after `return`.
- Template attribute order, normalized by hand in review: `*structuralDirective`, `#templateRef`,
  `booleanAttr`, `stringAttr="value"`, `[input]="value"`, `[(twoWay)]="value"`,
  `(output)="handler($event)"`.

### The upgrade playbook (how framework bumps are executed)

1. **One major version per commit**, even when done the same day (19 → 20 → 21 as three
   commits).
2. **Run the official schematic, then treat its output as debt** — codemod wrappers
   (`$safeNavigationMigration(...)`) are "unnecessary" and get deleted.
3. **A bump is a license to delete**: v19 deletes `standalone: true`; OnPush-default deletes
   `changeDetection:`; zoneless deletes zone.js + polyfills + `eventCoalescing`; Taiga 5
   deletes the compat shims marked for it.
4. **Replace your utilities with the library's on every bump**: their `LoadingService` →
   `TuiNotificationMiddleService`; `TUI_IS_MOBILE` → `WA_IS_MOBILE`; experimental
   `TuiDialogService` → `TuiResponsiveDialogService`; `provideAnimations()` +
   `provideEventPlugins()` → `provideTaiga({ mode })`.
5. **Adopt brand-new primitives immediately**: `linkedSignal`, `@let`, `TUI_BREAKPOINT`,
   `@Service()`, `text-wrap: balance` all appear in the same era they shipped.
6. **Docs ride along** — a dep-bump commit updates every doc fact it invalidates, in the same
   commit.
7. Taiga stays **pinned exact**; Angular gets carets. Don't bump Taiga majors/minors without
   the maintainer.
