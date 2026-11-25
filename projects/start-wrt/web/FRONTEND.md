# FRONTEND.md — StartWRT

Facts the `start9-frontend` skill needs about the StartWRT UI. Adds to the
[repo-root `FRONTEND.md`](../../../FRONTEND.md). Setup, `config.json`, the RPC transport and
the reference route folder are in [`AGENTS.md`](AGENTS.md); the API layer is in
[`ARCHITECTURE.md`](ARCHITECTURE.md).

## Change detection

Zoneless.

## Theme

Light and dark. Tokens in `src/styles.scss` sit under both `[tuiTheme='dark']` and
`:root, [tuiTheme='light']`; the user picks in Settings › General, which sets the
`TUI_DARK_MODE` signal at runtime.
`provideTaiga({ scrollbars: 'native' })`.

## i18n

Local dictionaries in `src/app/i18n/`; how they meet the shared ones is in `AGENTS.md`.
Validation messages route through i18n with `provideTranslatedValidationErrors`. Check:
`npm run check:i18n:wrt`.

## Backend

Signed-DH RPC typed from its own generated bindings, not `@start9labs/start-core`, and its
mock and error surfacing: `AGENTS.md`. For the skill's patterns:

- Pages read a polling `FormService<T>` (5 s), provided per page with `provideFormService(Impl)`.
- The action wrapper is `ActionService.run(action, { loading, success, fail, restart })`, not
  `TaskService`.
- A new endpoint follows the cross-frontend/backend rule in
  [`../AGENTS.md`](../AGENTS.md).

## Verify

`npm run check:wrt`, `npm run check:i18n:wrt`, `npm run build:wrt`; mock dev server
`npm run start:wrt`.

## Rules

- Icons are an inline registry (`loader: { ".svg": "text" }` + `app.icons.ts`,
  `tuiIconsProvider(ICONS)`) to keep the embedded bundle small. Keep the set complete, including
  icons Taiga components draw internally.
- The four same-path `''` routes (wizard, setup, app, login) discriminate by `canMatch` alone.
- Dialogs attach contextual help with `hostDirectives: [ModalHelp]`.
