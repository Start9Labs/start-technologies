# FRONTEND.md — start-technologies

Facts the [`start9-frontend`](.claude/skills/start9-frontend/SKILL.md) skill needs about every
Angular + Taiga app in this repo. Each app's own file adds to this one:
[ui](projects/start-os/web/ui/FRONTEND.md),
[setup-wizard](projects/start-os/web/setup-wizard/FRONTEND.md),
[start-tunnel](projects/start-tunnel/web/FRONTEND.md),
[start-wrt](projects/start-wrt/web/FRONTEND.md),
[brochure-marketplace](projects/brochure-marketplace/FRONTEND.md).

## Workspace

- One Angular workspace rooted at the repo root (`angular.json`, `package.json`,
  `tsconfig.json`); every app is a project in it and upgrades in lockstep. Install, dev-server,
  proxy and `config.json` setup are in
  [`shared-libs/ts-modules/AGENTS.md`](shared-libs/ts-modules/AGENTS.md). `npm ci` then
  `npm run build:deps` must run before anything type-checks.
- Apps read the gitignored repo-root `config.json` synchronously, through
  `require('<relative path>/config.json') as WorkspaceConfig`. Its `useMocks` drives the
  Live/Mock swap. start-wrt and brochure-marketplace read other files; see theirs.
- Every UI but brochure-marketplace is embedded into a Rust binary at compile time, so the web
  build precedes cargo; build products through `make`.
- A new app is a root `angular.json` project pointing into `projects/<product>/web`, with
  `start:x`, `build:x` and `check:x` scripts in the root `package.json`. Its styles layer
  `@taiga-ui/styles/taiga-ui-theme.less` → `shared-libs/ts-modules/shared/styles/taiga.scss` +
  `shared.scss` (brand apps) → the app's `styles.scss`. Copy start-wrt, the newest app.
- Icons are the asset copy (`node_modules/@taiga-ui/icons/src` → `assets/taiga-ui/icons`),
  except start-wrt.

## Theme

`shared-libs/ts-modules/shared/styles/taiga.scss` is the Start9 brand sheet: the dark theme as a
`[tuiTheme='dark']` block (backgrounds, statuses with `color-mix()` pale variants, text tiers),
Hanken Grotesk `@font-face` + `--tui-typography-family-*`, the custom
`[tuiAppearance][data-appearance='primary-success']`, and upstream shims pruned as Taiga absorbs
fixes. The global `g-*` utilities live in `shared.scss`.

## i18n

Every app translates; every user-facing string goes through `| i18n` and exists in all five
dictionaries (`en_US`, `de_DE`, `es_ES`, `fr_FR`, `pl_PL`). The shared dictionaries in
`shared-libs/ts-modules/shared/src/i18n/dictionaries/` serve ui, setup-wizard and
brochure-marketplace; start-tunnel and start-wrt keep local dictionaries on the same machinery,
pending consolidation into `shared`. A shared component translates through the shared `I18N`
token. Procedure: [`shared-libs/ts-modules/AGENTS.md` § Translations](shared-libs/ts-modules/AGENTS.md#translations).
Check: `npm run check:i18n`, plus `check:i18n:tunnel` and `check:i18n:wrt`.

## Backend

- Generated, typed JSON-RPC. Components and pages call the product's `ApiService`, whose
  methods are typed from the generated `RPC.*` trees in `@start9labs/start-core` (start-wrt
  generates its own); they never call the transport directly.
- StartOS and StartTunnel push state through PatchDB: websocket revisions → `bufferTime(250)` →
  `PatchDB`; pages read `toSignal(patch.watch$('key', 'path'))`.
- Transport, key admission, PatchDB fences and stream lifetimes belong to the shared primitives
  and the product API, auth, data-source and reconnect services. Their contracts are in
  [`shared-libs/ts-modules/ARCHITECTURE.md`](shared-libs/ts-modules/ARCHITECTURE.md); read it
  before changing one.
- Errors: `RpcError` (application, `isRpcError(body)`) and `HttpError` (transport); network loss
  is `{ code: 0 }`.
- Action wrapper: `TaskService.run(task, 'Saving')` from `@start9labs/shared` opens a
  `TuiNotificationMiddleService` loader, catches into `ErrorService` (negative toast, code-0
  network mapping, optional support link) and returns `Promise<boolean>`;
  `TaskService.runInline(task)` skips the loader.
- Dialog wrapper: `DialogService` (`openPrompt`/`openConfirm`/`openAlert`/`openComponent`,
  i18n-typed labels, escapes every `label`). Components never call
  `TuiResponsiveDialogService` directly.
- Dev backend: `MockApiService` when `useMocks` is true.

## Shared libraries

Before writing a utility or component, look in these.

`@start9labs/shared`:

- **Transport and auth** — `HttpService`, `RpcSocketService`, `AuthKeyService`,
  `ServerIdentityService`, `localRpcRequest`, `databaseSource`.
- **Actions and errors** — `TaskService`, `ErrorService` + `getErrorMessage` /
  `isNetworkError`, `HttpError`/`RpcError`.
- **Overlays** — `DialogService`, `CopyService` (clipboard + toast), ready
  `PolymorpheusComponent` dialogs `MARKDOWN` and `PROMPT`.
- **i18n** — `i18nService`/`i18nPipe`/`LocalizePipe` + `I18N_PROVIDERS`.
- **Components and directives** — `InitializingComponent`, `LogsWindowComponent`, `CaWizard`
  (+`CA_TRUST_CHECK`), `DocsLinkDirective` (+`VERSION`, optional), `SafeLinksDirective`,
  `IconDirective` (abstract base for an async icon `<img>`: the subclass supplies `src`, null
  until resolved; it owns the fallback and exposes `loading` for the avatar's `[tuiSkeleton]`;
  `FALLBACK_ICON`), `SetupLogsService` + `provideSetupLogsService(Api)`, `DownloadHTMLService`.
- **Pipes and utils** — `convertBytes`, `empty`, `compareExver`, `leafProgress`, `markdown`;
  `escapeHtml`, `Exver`, `convertAnsi`, `formatProgress`, `getPkgId`, `pauseFor`,
  `withAbortSignal`, `@debounce`, `sameUrl`, `isValidHttpUrl`, `registryUrl`,
  `hostnameValidator`, `hostnameValidationErrors`, `randomHostname`, keyboards/languages data,
  `defaultRegistries`/`knownRegistries`, the `RELATIVE_URL` token.

`@start9labs/marketplace`: the whole storefront kit (shell, tile, preview drawer, about/
release-notes/flavors/dependencies/links, registry picker) abstracted over
`AbstractMarketplaceService`. Apps provide the service implementation, inject optional hooks
(`MARKETPLACE_REGISTRY_ALERTS`), and pass install buttons as templates
(`contentChild(TemplateRef)`).

`@start9labs/start-core`: generated serde-aware types (`T.*`) and server method trees
(`RPC.StartOS`, `Setup`, `Init`, `Diagnostic`, `Registry`, `Tunnel`, `Effects`), with
`RPC.RpcMethod`, `RpcParamType` and `RpcReturnType` to select a method's types; `IST`/`ISB`
input-spec types/builders, `VersionRange`/`ExtendedVersion`, `S9pk`, utils, zod re-export.
Requests use the input declarations; output DTOs may contain required nullable fields absent
from inputs. Never hand-edit `osBindings/*.ts`: change the owning Rust schema, run
`make start-core-ts-bindings`, then rebuild core and SDK before checking consumers.

Apps consume `shared` and `marketplace` by tsconfig path alias to source; `start-core` is a
built `file:` dependency (`npm run build:deps`).

## Verify

`npm run check` (or `check:<app>`), the i18n checks above, `make web-format-check`, and the
app's production `npm run build:<app>`.
