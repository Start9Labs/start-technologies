# FRONTEND.md — StartOS ui

Facts the `start9-frontend` skill needs about the StartOS admin UI. Adds to the
[repo-root `FRONTEND.md`](../../../../FRONTEND.md); build and deploy are in
[`projects/start-os/AGENTS.md`](../../AGENTS.md).

## Change detection

Zone-based: `provideZoneChangeDetection()` with the `zone.js` polyfill; zoneless is pending.

## Theme

Dark, `provideTaiga({ mode: 'dark' })`, on the shared brand sheet.

## i18n

Shared dictionaries.

## Backend

JSON-RPC + PatchDB push. The router defers initial navigation
(`withDisabledInitialNavigation()`) until services initialize inside `provideAppInitializer`.
`utils/title-resolver.ts` composes "server — page" titles.

## Verify

`npm run check:ui`, `npm run build:ui`; mock dev server `npm run start:ui`.

## Rules

- The spec-driven dynamic form engine (`services/form.service.ts`) uses `UntypedFormBuilder`;
  it is the sanctioned exception to typed forms.
- Text a package authors (spec descriptions, warnings, footnotes and pattern descriptions,
  action descriptions and warnings, a long description) renders as sanitized markdown.
- `TUI_DIALOGS_CLOSE` is overridden in `app.config.ts` to also close dialogs on a server crash.
- `FormDialogService` guards unsaved changes with `TuiConfirmService.withConfirm`.
- Imports are `src/`-absolute.
