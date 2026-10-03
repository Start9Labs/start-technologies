# Shared libraries — @start9labs/\*

`@start9labs/shared` (monorepo apps): `HttpService` (JSON-RPC), `ErrorService` +
`getErrorMessage`, **`TaskService.run`** (the async-action wrapper), **`DialogService`**
(responsive + i18n prompt/confirm/alert/component), `CopyService` (clipboard + toast),
`DownloadHTMLService`, `Exver` (version algebra façade), `SetupLogsService` +
`provideSetupLogsService(Api)`, `i18nService`/`i18nPipe`/`LocalizePipe` + `I18N_PROVIDERS`,
`MarkdownComponent`/`MARKDOWN` + `PromptModal`/`PROMPT` (ready `PolymorpheusComponent` dialogs),
`InitializingComponent`/`LogsWindowComponent`, `CaWizard` (+`CA_TRUST_CHECK`),
`DocsLinkDirective` (+`VERSION`, optional),
`SafeLinksDirective`, pipes (`convertBytes`, `empty`, `compareExver`, `leafProgress`,
`markdown`, `trustUrl`), `RELATIVE_URL` token, `HttpError`/`RpcError`, disk/RPC/http types,
utils (`convertAnsi`, `formatProgress`, `getPkgId`, `pauseFor`, `@debounce`, `sameUrl`,
`isValidHttpUrl`, `registryUrl`, `hostnameValidator`, `hostnameValidationErrors`,
`randomHostname`, keyboards/languages data,
`defaultRegistries`/`knownRegistries`).

`@start9labs/marketplace`: the whole storefront kit (shell, tile, preview drawer, about/
release-notes/flavors/dependencies/links, registry picker) abstracted over
`AbstractMarketplaceService` — apps provide the service impl, inject optional hooks
(`MARKETPLACE_REGISTRY_ALERTS`), and pass install buttons as templates
(`contentChild(TemplateRef)`).

`@start9labs/start-core`: generated serde-aware types (`T.*`) and server method trees
(`RPC.StartOS`, `Setup`, `Init`, `Diagnostic`, `Registry`, `Tunnel`, `Effects`),
`IST`/`ISB` input-spec types/builders, `VersionRange`/`ExtendedVersion`, `S9pk`, utils,
zod re-export. `RPC.RpcMethod`, `RpcParamType` and `RpcReturnType` select request and
result types from method literals. Use input declarations for requests; output
DTOs may contain required nullable fields absent from inputs. **Never hand-edit
`osBindings/*.ts`** — change the owning Rust schema and run `make start-core-ts-bindings`,
then rebuild core and SDK before checking consumers.

StartWRT uses its own generated `Api` in `services/api/bindings.ts` and streaming
schemas in `events.ts`; run `make start-wrt-rpc-bindings`. Keep its aborting
transport. Shared `HttpService` remains a JSON-RPC transport boundary; product
wrappers infer results rather than accepting caller-selected return types.

Library authoring (when you add to `shared`/`marketplace`): configurability layers in order —
signal inputs → content projection/`contentChild(TemplateRef)`/`PolymorpheusContent` → abstract
class as DI contract → optional hook tokens → `provide*` factories → Taiga option providers.
Style with `--tui-*` vars, `:host { display: contents }` for a component composing several
siblings (one wrapping a single Taiga primitive becomes it via `hostDirectives`); no theme
definitions inside components.
