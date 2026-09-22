# Styling — aim for zero

The fleet's newest whole apps run on 12–48 lines of global CSS and a few `:host` blocks. The
ladder from SKILL.md doctrine #3, expanded:

1. **Layout primitives before any CSS**: `tuiCardLarge` (+`appearance="floating"`), `tuiHeader`
   - `hgroup tuiTitle` + `aside tuiAccessories` for page headers, `tuiCell` rows, `tuiForm="m"`,
     `tuiTitle`/`tuiSubtitle`, `tuiBadge`, `tuiAvatar` (large icons — not `tui-icon` +
     `font-size`), `TuiNavigation` shells, `tuiTable`. _"Hand-centering a card's contents means
     you missed a primitive."_
2. **Appearances are the color system**: `appearance="primary | secondary | flat | outline |
accent | positive | negative | warning | info | icon | floating | primary-destructive |
flat-grayscale | outline-grayscale | action-grayscale | …"` on buttons/badges/cells/
   notifications — never color classes on components that have appearances. `appearance=""`
   deliberately neutralizes chrome (e.g. accordion/drawer buttons). Custom appearances are
   built with Taiga's SCSS mixins in the theme sheet under
   `[tuiAppearance][data-appearance='…']`, not with bespoke selectors.
3. **Option providers restyle subtrees** — `providers: [tuiButtonOptionsProvider({ appearance:
'flat-grayscale', size: 'm' })]` on a header restyles every nav button, zero CSS. This is
   _the_ answer to "all the Xs in here should look like Y".
4. **Design tokens** for one-off values: `var(--tui-…)` everywhere; define new knobs as CSS
   custom properties bound from the template (`[style.--gap.rem]`).
5. **`g-*` global utilities** (shared stylesheet only — never re-declared per app/component):
   layout (`g-page`, `g-form`, `g-table`, `g-aside`, `g-buttons`, store's `g-band`/`g-wrap`
   marketing system) and text colors (`g-positive/negative/warning/info/secondary/primary`) for
   things with no appearance input (`tui-icon`). Their `!important` is by design; nowhere else.
6. **`:host` layout CSS last**: `display: grid/flex`, `gap`, sizing. Modern CSS is expected —
   logical properties (`inline-size`, `margin-block`, `inset-inline-start`), `:has()`,
   `color-mix()`, `clamp()`, container queries, `dvh`. Fluid sizing via `min(36rem, 90vw)`.
   Taiga SCSS utils import into inline styles: `@use '@taiga-ui/styles/utils' as taiga` +
   `@include taiga.transition(...)`.

Responsive: **no `@media` for the app-standard mobile swap.** CSS: `tui-root._mobile &` /
`:host-context(tui-root._mobile)`. TS/template: `inject(TUI_BREAKPOINT)` / `WA_IS_MOBILE` +
`@if` — render **one DOM**, restyled or swapped per breakpoint; duplicated `.desktop`/`.mobile`
DOM pairs get rewritten (the CSS-only responsive table: cells carry `[attr.data-label]`, mobile
restyles with `td[data-label]::before { content: attr(data-label) ': ' }`). A component that
must diverge from the app threshold uses its own `@media` with a comment saying so.

**A child carries no layout for its parent.** Where a reusable component sits (a resizer's
`inset-inline`) is the parent's rule, not a `side="start"` variant on the child; a shell that
wants its routed pages to flow into its card says so once,
`::ng-deep > :last-child { display: contents }`, not through a `g-*` class every page puts on
its host. Hiding an element on phones is `display: none` under
`:host-context(tui-root._mobile)`; an `@if` on the breakpoint is for branches that differ.

A grid or flex item whose `overflow` isn't `visible` already has a zero minimum size, so
`min-inline-size: 0` beside it is noise. Never `outline: none` — the focus ring stays.

`::ng-deep`: tolerated only at shell seams (piercing `tui-scrollbar` internals, a shell laying
out its routed child, styling `[innerHTML]` content) — never component-to-component.
`!important`: `g-*` utilities and documented Taiga-collision one-offs only. Both are review
flags otherwise.
