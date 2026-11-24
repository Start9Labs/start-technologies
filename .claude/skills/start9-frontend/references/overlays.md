# Overlays — dialogs, toasts, dropdowns, hints

- **Dialogs — `TuiResponsiveDialogService`** (desktop dialog ↔ mobile sheet automatically; the
  monorepo wraps it as `DialogService` in `@start9labs/shared` with i18n-typed
  `openPrompt/openConfirm/openAlert/openComponent` — components there never call Taiga's service
  directly). Flow:

```ts
// caller
this.dialogs
  .open<Result>(new PolymorpheusComponent(PublishPortDialog), { data: { devices, existing } })
  .subscribe(async result => { await this.service.save(result) })

// dialog component (no selector)
protected readonly context = injectContext<TuiDialogContext<Result, Data>>()
// cancel: this.context.$implicit.complete()   confirm: this.context.completeWith(result)
```

A component dialog titles itself: `<header tuiHeader><h2 tuiTitle [id]="context.id">`, and the
same `[id]` on the heading of any state that replaces it (a load failure's `tui-block-status`).
The dialog is labelled by its own heading, and callers pass no `label`. `label` is for dialogs with no
component of their own (confirms, prompts).

Reusable dialogs export a ready const: `export const PROMPT = new
  PolymorpheusComponent(PromptModal)` at the bottom of the dialog file. Confirmations use kit's
`TUI_CONFIRM`: `.open(TUI_CONFIRM, { label, data }).pipe(filter(Boolean)).subscribe(...)`.
Simple local dialogs may be declarative: `<ng-template [(tuiDialog)]="open">…` bound to a
`signal(false)`, options via `[tuiDialogOptions]`; custom widths hook the global sheet via
`data-appearance` token-matching, not `::ng-deep`. Not used: routed dialogs, the `tuiDialog()`
component-wrapper helper.

A component that is also a page takes its data as inputs, not `injectContext`. The outlet sets
every input a context key names, so `.open(DETAILS, { appDetails: id })` and
`*polymorpheusOutlet="details; context: { appDetails: id }"` render one component as dialog and
page, with no wrapper dialog; the extra key needs the options cast to
`Partial<TuiResponsiveDialogOptions>`.

- **Toasts — `TuiNotificationService`** (`.open(msg, { appearance: 'positive' | 'negative', …
}).subscribe()` fire-and-forget; `autoClose: 0` + `closable: false` for sticky states, content
  can be a `PolymorpheusComponent`). **Blocking loaders — `TuiNotificationMiddleService`**: hold
  the subscription open, `unsubscribe()` in `finally` (that's what `TaskService` does).
  Inline banners are `<div tuiNotification appearance="…">` (host-directive form, not the
  element form). A banner that asks for one of
  two choices is a `form`: the primary action is its default submit button and the alternative
  a `type="reset"` button, handled as `(submit.prevent)` and `(reset)` on the form, with no
  `(click)` or `type="button"` on either.
- **Dropdowns**: `tuiDropdown` + `tuiDropdownAuto`/`tuiDropdownHover`/`tuiDropdownOpen`, content
  `<tui-data-list *tuiDropdown="let close"><button tuiOption (click)="close()">…` inside the
  host element — the context-provided `close`; `tuiOption` stamps `type="button"` itself. A menu
  is `tuiDropdown tuiDropdownAuto`; an `open = signal(false)` behind `[(tuiDropdownOpen)]`, set
  back to `false` in every handler, is the rewrite target. A menu two triggers share is a
  component, closed once from its host: `<app-menu *tuiDropdown="let close" (click)="close()" />`.
  A trigger at the inline end of its row or bar — row actions, a header menu — sets
  `tuiDropdownAlign="end"`: an anchored dropdown wraps to the room past its aligned edge rather
  than flipping. Leave `tuiDropdownDirection` unset: its default opens on whichever side has
  room, and a pinned `top` breaks when there's none. A menu that a phone should open as a
  bottom sheet adds `tuiDropdownSheet` (`@taiga-ui/addon-mobile`) to the same host; it swaps
  on the device (`WA_IS_MOBILE`), not the breakpoint, and the content's `close` still closes it.
- **Hints**: `[tuiHint]` (template content allowed), tuned globally via `tuiHintOptionsProvider`.
  It goes on the control, `<button [tuiHint]="…"><tui-icon … /></button>`, never on the icon
  inside it, so the whole control shows it.
- **Text from outside the OS never reaches Taiga as a raw string.** Taiga renders string
  content — hint, dialog `label` and content, confirm, notification, `tui-error` — through
  `[innerHTML]`. Text a package authors (spec descriptions, warnings, footnotes and pattern
  descriptions, action descriptions and warnings, a long description) renders as sanitized
  markdown, through the triple in components.md, in a template or component:
  `<tui-icon [tuiTooltip]="tooltip"><ng-template #tooltip><div class="g-markdown" safeLinks [innerHTML]="hint | markdown | dompurify"></div></ng-template></tui-icon>`
  — nested, so an `@if` around it keeps a single root and the icon its textfield slot. Where
  only a string fits (`tui-error`), pass dompurified markdown and put `safeLinks` on the host.
  A value the OS interpolates into its own message — a name, a title, a hostname, error text
  — is plain text: interpolate it, or `escapeHtml` it inside OS-authored dialog markup, and
  escape a templated label such as `displayAs` whole, after rendering. `DialogService`
  escapes every `label` itself.
- **Drawers/sheets**: `<tui-drawer *tuiPopup="open()" (click.self)="toggle(false)">` with
  URL-driven `open` state.
- Dialogs close on route activation — `TUI_DIALOGS_CLOSE`'s default — so a dialog doesn't watch
  the router to close itself. The exception is a mobile sheet opened with `closable: false`,
  which gates the token through `closable` and stays open. StartOS overrides the token to close
  on a server crash too: app-level policy expressed as one token override.
