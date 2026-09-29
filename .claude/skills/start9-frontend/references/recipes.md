# Recipes

**Add a routed page**

1. `routes/<name>/index.ts`: selectorless `@Component` + `export default class <Name>`.
2. Route entry: `{ path: '<name>', loadComponent: () => import('./routes/<name>') }`.
3. Data via the app's service pattern (state.md); template three-arm `@if`; `host: { class: 'g-page' }`
   where the app uses it. i18n every string (monorepo).

**Add a dialog**

1. `routes/<feature>/dialog.ts`: selectorless component; `protected readonly context =
injectContext<TuiDialogContext<Result, Data>>()`; form via NNFB; footer with flat Cancel
   (`context.$implicit.complete()`) and submit (`context.completeWith(result)`).
2. Long copy goes in `data`/content, not the label.
3. Caller: `this.dialogs.open<Result>(new PolymorpheusComponent(MyDialog), { label, data })
.subscribe(result => …)` — through the shared `DialogService` in monorepo apps.
4. Per-dialog validation messages via `tuiValidationErrorsProvider` (or the app's translated
   wrapper) in the dialog's `providers`.

**Add a form** — follow forms.md top to bottom.

**Add a table page** — `table[appX]` component with
`hostDirectives: [{ directive: TuiTableDirective, inputs: ['sorter'] }]` or plain `<table>` +
`tuiTh`; `@for (… track item.id)`; `[tuiSkeleton]` while loading; row actions as `tuiDropdown`

- `<tui-data-list *tuiDropdown="let close">`; mobile via self-labeling cells
  (`[attr.data-label]`) restyled under `:host-context(tui-root._mobile)` — never a second DOM.

**Create a reusable control component** — components.md control-component rule: attribute selector on the semantic element,
`hostDirectives`, static attrs in `host: {}`, config via option providers/`TUI_ICON_START`,
selector-aliased `input.required`, styles on `:host` only.

**Add an app to the monorepo / stand up a new repo** — bootstrap.md. Copy the newest sibling
(start-wrt in-monorepo; support-server standalone), not the oldest.

**Restyle something** — walk the ladder in SKILL.md/styling.md and stop at the first rung that works.
If you're about to write CSS, say which rungs you ruled out and why.

**Add an i18n string** — conventions.md: template `| i18n`, `en.ts` + all dictionaries, run the check.

**Add a backend endpoint** — extend the abstract `ApiService` + both `LiveApiService` and
`MockApiService` (skipping the mock breaks `npm run start:*`); types from `T.*`/shared
schemas; StartWRT also updates `API_CONTRACT.md` and the Rust handler in the same change;
start9-store follows its shared→api→web chain (`shared/src` types + Zod → `CommerceBackend` →
route → `ApiClient` method).
