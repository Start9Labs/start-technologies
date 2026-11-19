# Antipattern catalog — what gets rewritten in review

The before→after pairs review keeps producing. The recurring diagnosis: _someone hand-built a
thing the platform already provides, and the fix is to find the primitive, configure it once
via DI, and delete the rest._ Where a rule has a home in another reference, the entry names it
instead of restating it.

**1. Repeated styling attributes / CSS overrides of Taiga → option provider** (styling.md, rung 3).

```html
<!-- BEFORE: appearance="flat" repeated on 8 nav buttons; or CSS:
     a[tuiLink] { color: var(--tui-text-secondary); font: var(--tui-typography-body-s) } -->
```

```ts
// AFTER: one line, zero CSS
providers: [tuiButtonOptionsProvider({ appearance: 'flat-grayscale', size: 'm' })]
providers: [tuiLinkOptionsProvider({ appearance: 'action-grayscale' })]
```

**2. Hand-rolled card/heading/list CSS → layout primitives** (styling.md, rung 1). Custom
`.card`/`.head` divs become `<section tuiCardLarge="compact" appearance="secondary-grayscale">`
with `<header tuiHeader><h3 tuiTitle>` and `<ul tuiList="s">`. A `role="table"` div grid becomes
`tuiCell` rows over `| keyvalue`; label/value pairs are a `<dl tuiList>` of `dt`/`dd`
(`compact` when dense), not `tuiSubtitle` stacks. The pieces inside go the same way:

```html
<!-- BEFORE: ._avatar circle, ._chip pill, styled <code>, tui-icon { color } — ~80 lines of CSS -->
<article tuiCardLarge="compact" appearance="floating">
  <header tuiHeader="body-s">
    <span tuiAvatar="@tui.shield-alert" appearance="negative"></span>
    <span tuiTitle>
      Notice of a security incident
      <small tuiSubtitle>Your vendor's security team</small>
    </span>
    <aside tuiAccessories>
      <span tuiBadge appearance="positive" iconStart="@tui.lock">StartTunnel</span>
    </aside>
  </header>
  <p tuiDescription>…</p>
</article>
<span tuiChip iconStart="@tui.eye">Vendor</span>
<code tuiChip size="xs">start-cli</code>
```

**3. Per-component loader + try/catch + error toast → `TaskService.run(task, 'Saving')`**
(state.md). A flow that needs raw promise semantics keeps a manual loader; the wrapper is not
forced where it doesn't fit.

**4. Duplicated desktop/mobile DOM → one DOM restyled per breakpoint** (styling.md, Responsive).

**5. Wrapper elements → attribute components on semantic hosts** (components.md, Selectors). A
`.shell` wrapper div is deleted in favor of semantic elements directly inside `tui-root`.

**6. Semantic-HTML repair.** `<h3>` abused for body text (with CSS undoing it) → plain text
sized by `tuiHeader="body-l"`; `<h2>` for dialog sections → `<h3 tuiHeader="h6">` (heading
_level_ decoupled from visual _size_); `<h1>`+`<p>` → `<hgroup tuiTitle>` + `tuiSubtitle`;
click-handler navigation → real `<a tuiButton [href]>`; icon-only buttons get text content
(Taiga hides it visually) instead of `aria-label`; bare `<label>` + flex CSS →
`<label tuiLabel>`; styled `<span class="divider">` → `<hr>`; `autocomplete="new-password"` on
password fields; `TuiAutoFocus` on the first focusable in dialogs/drawers, never on phones. A
field that collapses behind a toggle stays in the tab order and expands on
`(focus)="open.set(true)"`. It doesn't drop out with `aria-hidden` + `tabIndex="-1"`.

**7. Fine-tuning CSS → deleted.** Every `letter-spacing` and `text-transform` nudge and every
`margin-top` ladder goes; vertical rhythm comes from the primitives' own spacing plus one margin
on the element that needs more (styling.md). Physical properties → logical (`inset-block-start`,
`inline-size`, `margin-block-end`) in every touched file.

**8. Specificity hacks → deleted.** `!important` globals get scoped without it; `::ng-deep`
layout pokes die with the primitive that replaces them; custom appearance CSS
(`[tuiCardLarge] { background: … }`) → `appearance="floating"`. When a global override _is_
warranted, double the selector (`.g-negative.g-negative { … }`,
`tui-dropdown[data-appearance='start-os'][data-appearance='start-os']`).

**9. Form ceremony → `NonNullableFormBuilder` shorthand** (forms.md), with single-use private
fields inlined into the group literal.

**10. Single-use names, wrapper methods, multi-branch returns → inlined expressions.**
`select(type) { this.context.completeWith(type) }` + `(click)="select('public')"` →
`(click)="context.completeWith('public')"`; aliased fields and wrapper methods over a service →
the service itself (components.md, member conventions). A module-level
`const BULLET: Record<Status, string>` that one component reads becomes that component's field.
A helper takes the value it needs, not the injectable that produces it:
`pickTopic(dialogs, i18n.transform('Choose a topic'))`, not `pickTopic(dialogs, i18n)`.
`if`-ladders → one boolean expression or ternary; `p.length > 0` → `!!p.length`; flag
parameters → default parameters; `try/catch` around a subscribe callback → deleted.

**11. Derived-state ceremony → template pipes.** A `computed()` mapping
`Object.entries(x).map(([name, value]) => ({name, value}))` → `@for (row of x | keyvalue)`;
a `switchMap(async …)` that builds an object → `map(c => c ?? defaultConfig)`.

**12. Manual reset plumbing → `linkedSignal`** (components.md, Signal APIs). Conversely,
needless signal ceremony gets deleted (a `TitleStrategy` injection where
`toSignal(router.events)` as a change trigger sufficed).

**13. DI style.** `private readonly patch: PatchDB<DataModel> = inject(PatchDB)` →
`inject<PatchDB<DataModel>>(PatchDB)`; inject the **narrow** dependency (`PATCH_CACHE`
observable, not the whole `PatchDB`) to break cycles; type against the abstract class; a child
may inject its parent component instance (`inject(MarketplacePreviewComponent)`) instead of
prop-drilling; hand-rolled unions → library types (`PolymorpheusContent`).

**14. Dead code dies immediately.** Unused components, `.html`/`.scss` orphans,
`.asObservable()` no-ops, compat shims once the upgrade they waited for lands, unused imports,
font families the design no longer uses.

**15. Utility-class hygiene.** `g-*` utilities exist once in the shared/global sheet — per-app
copies get centralized, one-off spacing classes (`.padding-top`) get deleted. A global modifier
that only one component uses (`.g-markdown._document`) moves into that component as
`:host ::ng-deep .g-markdown { … }`. A positional rule in a global utility goes inside
`:where()`, so a component's `:host` still wins: `.g-section:where(:nth-of-type(even))`.

**16. Copy-pasted markup branches → data-driven rendering.** Hand-written dropdown templates and
nav buttons → one `navigation` object rendered via `| keyvalue: asIs` (`asIs = () => 0` keeps
insertion order); string-vs-object value discriminates leaf vs dropdown; a `/` prefix
discriminates `routerLink` vs external `href`.

**17. Copy is sentence case.** `'Beginning Backup'` → `'Beginning backup'`. Title Case only
for proper nouns and page titles.

### Review quotes (verbatim)

- On template function calls: _"This way you call this function on each change detection,
  effectively creating a new Observable each time. You need a readonly property… or a custom
  pipe so that it is only called once."_
- On dialog copy: _"It makes more sense for longer text to be in `data: { content }` rather
  than in the title."_
- On error paths: _"Do we want to complete it on finally and not in successful try? What if we
  lose connection while entering the values — we would then lose the form."_
- On component size: _"It's easier to maintain when there's a single component per file"_ —
  tiny private helpers (a toast, a dialog's `PolymorpheusComponent` const) may co-locate.
- On RxJS: observables end in `$`; `defer(() => this.api.call().pipe(catchError(…)))` over
  fetch-on-init; drop `async`/`switchMap` when nothing is async — use `map`; no `else` after
  `return`.

### The upgrade playbook

1. **One major version per commit**, even when done the same day.
2. **Run the official schematic, then treat its output as debt** — codemod wrappers
   (`$safeNavigationMigration(...)`) get deleted.
3. **A bump is a license to delete**: v19 deletes `standalone: true`; OnPush-default deletes
   `changeDetection:`; zoneless deletes zone.js, polyfills and `eventCoalescing`; a Taiga major
   deletes the shims marked for it.
4. **Replace your utilities with the library's on every bump**: their `LoadingService` →
   `TuiNotificationMiddleService`; `TUI_IS_MOBILE` → `WA_IS_MOBILE`; `TuiDialogService` →
   `TuiResponsiveDialogService`; `provideAnimations()` + `provideEventPlugins()` →
   `provideTaiga({ mode })`.
5. **Adopt new primitives as they ship**: `linkedSignal`, `@let`, `TUI_BREAKPOINT`,
   `@Service()`, `text-wrap: balance`.
6. Version pinning and the fleet table's update are in SKILL.md, The fleet.
