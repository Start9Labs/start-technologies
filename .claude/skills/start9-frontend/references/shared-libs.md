# Shared libraries — @start9labs/\*

Before writing a utility or component in a monorepo app, check these libraries for it.

`@start9labs/shared` (monorepo apps):

- **Transport and auth** — `HttpService`, `RpcSocketService`, `AuthKeyService`,
  `ServerIdentityService`, `localRpcRequest`, `databaseSource`. Their contracts are in
  `shared-libs/ts-modules/ARCHITECTURE.md`.
- **Actions and errors** — **`TaskService.run`** (the async-action wrapper, state.md),
  `ErrorService` + `getErrorMessage` / `isNetworkError`, `HttpError`/`RpcError`.
- **Overlays** — **`DialogService`** (responsive + i18n prompt/confirm/alert/component,
  overlays.md), `CopyService` (clipboard + toast), ready `PolymorpheusComponent` dialogs
  `MARKDOWN` and `PROMPT`.
- **i18n** — `i18nService`/`i18nPipe`/`LocalizePipe` + `I18N_PROVIDERS` (conventions.md).
- **Components and directives** — `InitializingComponent`, `LogsWindowComponent`, `CaWizard`
  (+`CA_TRUST_CHECK`), `DocsLinkDirective` (+`VERSION`, optional), `SafeLinksDirective`,
  `IconDirective` (abstract base for an async icon `<img>`: the subclass supplies `src`, null
  until resolved; it owns the fallback and exposes `loading` for the avatar's `[tuiSkeleton]`;
  `FALLBACK_ICON`), `SetupLogsService` + `provideSetupLogsService(Api)`, `DownloadHTMLService`.
- **Pipes and utils** — `convertBytes`, `empty`, `compareExver`, `leafProgress`, `markdown`;
  `escapeHtml`, `Exver`, `convertAnsi`, `formatProgress`, `getPkgId`, `pauseFor`, `withAbortSignal`,
  `@debounce`, `sameUrl`, `isValidHttpUrl`, `registryUrl`, `hostnameValidator`,
  `hostnameValidationErrors`, `randomHostname`, keyboards/languages data,
  `defaultRegistries`/`knownRegistries`, the `RELATIVE_URL` token.

`@start9labs/marketplace`: the whole storefront kit (shell, tile, preview drawer, about/
release-notes/flavors/dependencies/links, registry picker) abstracted over
`AbstractMarketplaceService` — apps provide the service impl, inject optional hooks
(`MARKETPLACE_REGISTRY_ALERTS`), and pass install buttons as templates
(`contentChild(TemplateRef)`).

`@start9labs/start-core`: generated serde-aware types (`T.*`) and server method trees
(`RPC.StartOS`, `Setup`, `Init`, `Diagnostic`, `Registry`, `Tunnel`, `Effects`), with
`RPC.RpcMethod`, `RpcParamType` and `RpcReturnType` to select a method's types;
`IST`/`ISB` input-spec types/builders, `VersionRange`/`ExtendedVersion`, `S9pk`, utils, zod
re-export. Requests use the input declarations; output DTOs may contain required nullable
fields absent from inputs. **Never hand-edit `osBindings/*.ts`**: change the owning Rust schema
and run `make start-core-ts-bindings`, then rebuild core and SDK before checking consumers.
StartWRT's generated `Api` lives in its own `services/api/bindings.ts` and `events.ts`;
regenerate them with `make start-wrt-rpc-bindings`.

Library authoring (when you add to `shared`/`marketplace`): configurability layers in order —
signal inputs → content projection/`contentChild(TemplateRef)`/`PolymorpheusContent` → abstract
class as DI contract → optional hook tokens → `provide*` factories → Taiga option providers.
Style with `--tui-*` vars and keep theme definitions out of components.
