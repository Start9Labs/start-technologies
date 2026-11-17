# Recipes

Step lists that stitch the references together. Each step names where its rule lives.

**Add a routed page**

1. `routes/<name>/index.ts`: selectorless `@Component` + `export default class <Name>`
   (components.md).
2. Route entry: `{ path: '<name>', loadComponent: () => import('./routes/<name>') }`
   (routing.md).
3. Data via the app's service pattern and the three-arm `@if` (state.md); i18n every string in
   monorepo apps (conventions.md).

**Add a dialog** — overlays.md, Dialogs: a selectorless component in `routes/<feature>/dialog.ts`
with `injectContext`, a form per forms.md, its validation messages in the dialog's `providers`,
and long copy in `data` rather than the label.

**Add a form** — forms.md, top to bottom.

**Add a table page** — a `table[appX]` component with
`hostDirectives: [{ directive: TuiTableDirective, inputs: ['sorter'] }]`, or `<table tuiTable>`
with `th[tuiTh]`/`td[tuiTd]` (`@taiga-ui/addon-table`). Never hand-write cell padding, borders
or fonts.

- A body `th[tuiTd]` sticks to the inline start on its own, so it is the row header of a table
  that scrolls sideways (pair it with `<th tuiTh [sticky]="true">`); a column that shouldn't
  stick is `td[tuiTd]`. `tuiTable` needs `border-collapse: separate`, so never re-collapse it.
- `@for (… track item.id)`; `[tuiSkeleton]` while loading; row actions in a `tuiDropdown` with
  `<tui-data-list *tuiDropdown="let close">` (overlays.md).
- On mobile, cells label themselves (`[attr.data-label]`) and restyle under
  `:host-context(tui-root._mobile)` (styling.md, Responsive) — never a second DOM.

**Create a reusable control component** — components.md, Selectors.

**Add an app to the monorepo / stand up a new repo** — bootstrap.md. Copy the newest sibling
(start-wrt in-monorepo; support-server standalone), not the oldest.

**Restyle something** — walk the ladder in styling.md and stop at the first rung that works.
If you're about to write CSS, say which rungs you ruled out and why.

**Add an i18n string** — conventions.md.

**Add a backend endpoint** — extend the abstract `ApiService` and both `LiveApiService` and
`MockApiService` (skipping the mock breaks `npm run start:*`), with types from `T.*`/shared
schemas. StartWRT also updates `API_CONTRACT.md` and the Rust handler in the same change;
start9-store follows its `AGENTS.md` § Pattern: adding a new commerce endpoint.
