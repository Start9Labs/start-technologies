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

- **Toasts — `TuiNotificationService`** (`.open(msg, { appearance: 'positive' | 'negative', …
}).subscribe()` fire-and-forget; `autoClose: 0` + `closable: false` for sticky states, content
  can be a `PolymorpheusComponent`). **Blocking loaders — `TuiNotificationMiddleService`**: hold
  the subscription open, `unsubscribe()` in `finally` (that's what `TaskService` does).
  `TuiAlertService` is not used anywhere; inline banners are `<div tuiNotification
appearance="…">` (host-directive form, not the element form).
- **Dropdowns**: `tuiDropdown` + `tuiDropdownAuto`/`tuiDropdownHover`/`tuiDropdownOpen`, content
  `<tui-data-list *tuiDropdown="let close"><button tuiOption (click)="close()">…` inside the
  host element — the context-provided `close`. A menu is `tuiDropdown tuiDropdownAuto`; an
  `open = signal(false)` behind `[(tuiDropdownOpen)]`, set back to `false` in every handler, is
  the rewrite target. **Hints**: `[tuiHint]` (template content allowed), tuned globally
  via `tuiHintOptionsProvider`. **Drawers/sheets**: `<tui-drawer *tuiPopup="open()"
(click.self)="toggle(false)">` with URL-driven `open` state.
- All dialogs auto-close on navigation/server-crash in StartOS via a custom `TUI_DIALOGS_CLOSE`
  factory — app-level policy expressed as one token override.
