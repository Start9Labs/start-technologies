# Components & templates

### The shape

```ts
@Component({
  selector: 'button[appThing]', // attribute selector — or NO selector for routed/dialog components
  template: `
    @if (loading()) {
      <tui-loader size="s" />
    } @else {
      <span tuiTitle>
        <b>{{ item().title }}</b>
        <span tuiSubtitle>{{ item().version }}</span>
      </span>
    }
  `,
  styles: `
    :host {
      display: flex;
      gap: 0.5rem;
    }
  `, // layout only, tokens only — often absent
  host: { type: 'button', '[class._active]': 'active()' },
  hostDirectives: [TuiCell],
  providers: [tuiButtonOptionsProvider({ size: 's' })],
  imports: [TuiLoader, TuiTitle],
})
export class ThingComponent {
  private readonly api = inject(ApiService)

  readonly item = input.required<Item>({ alias: 'appThing' })
  protected readonly loading = signal(false)
}
```

- **One file.** Inline template, inline styles. Multiple small components/directives per file is
  fine (a dialog + its `PolymorpheusComponent` const; a toast component inside a service file).
- **Member conventions:** injected deps `private readonly`; template-facing members
  `protected readonly`; signal inputs/outputs `readonly` (public only when a parent binds them).
  No `public` keyword noise otherwise.
- **Routed and dialog components have no selector** (instantiated by router/Polymorpheus) and
  routed ones are `export default class` so `loadComponent: () => import('./x')` needs no `.then`.

### Selectors: the control-component rule

A component that **is** a control (button, link, badge, row, card, shell chrome) gets an
**attribute selector on the semantic element** plus `hostDirectives`, never a wrapper tag:

```ts
// YES — the component IS the element:
@Component({
  selector: 'a[marketplacePackageLink]',
  hostDirectives: [TuiButton],
  host: { tuiButton: '', target: '_blank', rel: 'noreferrer' },
  providers: [tuiButtonOptionsProvider({ size: 's', appearance: 'flat-grayscale' })],
  ...
})

// NO — a <my-thing> wrapper around a styled child with a
// :host { display: grid } stretch hack — these get unwrapped in review.
```

**Stamp the host directive's own attribute** (`host: { tuiButton: '' }`) — Taiga's CSS is keyed
on `[tuiButton]`, `[tuiChip]`, `[tuiBadge]`, and a host directive does not add its selector to
the element. Skip it and the directive runs with none of its styling, silently: no chrome, no
`iconStart`. `TuiCell` and `TuiCardLarge` stamp theirs; `TuiButton`, `TuiChip`, `TuiBadge` don't.

Real fleet examples: `header[appHeader]`, `footer[appFooter]`, `button[marketplaceTile]`
(+`TuiCardLarge`), `button[server]` (+`TuiCell`), `table[appTable]`
(+`{ directive: TuiTableDirective, inputs: ['sorter'] }`), `img[storeIcon]`,
`tr[appService]`, `ng-template[title]`. Element selectors are for true leaf widgets
(`check-icon`, `app-placeholder`); nobody enforces a prefix — clarity wins.

`hostDirectives` is also the **composition** mechanism — the purest exemplar (start-wrt):

```ts
@Directive({
  selector: '[formLoading]',
  host: { class: 'g-form', style: 'overflow: visible' },
  hostDirectives: [TuiForm, TuiCardLarge, { directive: TuiSkeleton, inputs: ['tuiSkeleton: formLoading'] }],
})
export class Form {}
```

One attribute = Taiga form + card + skeleton-on-loading, input forwarded. Twelve start-wrt
dialogs attach contextual help the same way: `hostDirectives: [ModalHelp]`.

### `host: {}` carries all host concerns

```ts
host: {
  class: 'g-page',                          // static class
  type: 'button',                           // static attr
  '(click)': 'toggle(true)',                // listener
  '[class._connected]': 'status() === "success"', // state class (Taiga-style _underscore)
  '[style.--plugins]': 'plugins.size() / 100',    // CSS custom property binding
  '[attr.inert]': '!open() || null',
}
```

`@HostBinding`/`@HostListener`: zero occurrences in target-style code.

### Signal APIs

- Inputs: `input()`, `input.required<T>()`, selector-aliased for attribute components
  (`input.required<Pkg>({ alias: 'marketplaceTile' })`). **Optional inputs get a real default —
  `input('')`, `input(false)` — not `input<T | null>(null)`** unless `null` is a meaningful
  value. `*ngTemplateOutlet` no-ops on null, so template inputs need no `@if` guard.
- Two-way: `model()` (`query = model('')`).
- Outputs: `output()` — rare; prefer `model()` or URL state.
- Queries: `viewChild()`, `contentChild()` — e.g.
  `contentChild<TemplateRef<{ $implicit: Pkg }>>(TemplateRef)` for app-supplied fragments.
- Derivation: `computed()`. Reset-on-source-change: `linkedSignal`:

```ts
// Drawer auto-closes on navigation or resize — zero per-link handlers:
protected readonly open = linkedSignal({
  source: () => [this.breakpoint(), this.route()],
  computation: () => false,
})
// Error clears when the user edits; re-mask when the secret changes:
readonly error = linkedSignal<string, i18nKey | null>({
  source: () => this.password(),
  computation: () => null,
})
```

- `effect()` is for **genuine external side effects** (syncing backend-stored theme/language to
  `TUI_DARK_MODE`, imperative DOM), never for deriving state — that's `computed`/`linkedSignal`.

### No lifecycle hooks

Work happens in field initializers. Fetch kickoff is a bare constructor statement
(`constructor() { this.load() }` / `void this.fetch()`); browser-only side effects use
`afterNextRender`; long-lived streams are **class-field subscriptions**:

```ts
readonly subscription = merge(inject(PatchDataService), inject(PatchMonitorService))
  .pipe(takeUntilDestroyed())
  .subscribe()
```

`takeUntilDestroyed()` is called bare in field initializers (injection context — no `DestroyRef`
argument). `ngOnDestroy` survives only for cleaning up manually created refs. An `ngOnInit`
doing form-patching is _known debt_, not style (`// TODO @Alex refactor this to declarative
validation`).

## Templates

- `@if (data(); as d) { … } @else if (error()) { … } @else { <tui-loader /> }` — the standard
  three-arm async template. `@for (item of items(); track item.id)` — always `track`
  (`$index` for static lists), `@empty` where useful. `@let` for local aliases. `@switch` over
  chained `@if` for enums.
- Templates read **signals** (`{{ name() }}`). `| async` survives only for service streams that
  are still Observables at the boundary; new code converts with `toSignal` in the class.
- **Class/style bindings, not directives**: `[class._expanded]="open()"`,
  `[style.margin-inline.rem]="0.5"`, `[style.--var]="x()"`. `ngClass`/`ngStyle`: banned.
- **Event-plugin modifiers** (shipped via `provideTaiga`): `(submit.prevent)="save()"`,
  `(click.self)="close()"`, `.stop`, `.capture`, `.once`, `.passive`.
- **Template reference variables replace trivial state**: `#input` +
  `(input)="onQuery(input.value)"`; `#carousel` + `carousel.next()`.
- **Attribute order** on an element: `*structuralDirective`, `#templateRef`, `booleanAttr`,
  `stringAttr="value"`, `[input]="value"`, `[(twoWay)]="value"`, `(output)="handler($event)"`.
- **The URL is component state** for anything shareable: drawers open when query params match
  (`?id=…&flavor=…`), pagination writes `queryParams` and a single `queryParamMap` subscription
  loops it back; search/registry/category selections live in query params (linkable + SSR-safe).
- i18n (monorepo apps): **every** user-facing literal goes through `| i18n` (or
  `inject(i18nPipe).transform(...)` in TS); backend `LocaleString`s through `| localize`.
- Rendered markdown is always the triple: `[innerHTML]="text | markdown | dompurify"` plus the
  `safeLinks` directive (forces `target="_blank" rel="noreferrer"` on external links).
- Text stacks use `tuiTitle`/`tuiSubtitle` with `<b>` for the title — never custom
  heading/caption CSS: `<span tuiTitle><b>{{ title }}</b><span tuiSubtitle>{{ sub }}</span></span>`.
