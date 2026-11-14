# Shared libraries — @start9labs/\*

`@start9labs/shared` (monorepo apps): `HttpService` (HTTP JSON-RPC/resources),
`RpcSocketService` (encrypted local RPC/continuations), `localRpcRequest` (OS/Tunnel local replies/fences),
`authenticatedRpcTransport` (device-key typed-stream admission), `AuthKeyService` (device-key and
unenrolled admission leases), `ServerIdentityService` (normal-product verified-peer observers),
`rpcWebSocketUrl` (shared RPC/continuation endpoint selection),
`databaseSubscription` / `databaseSource` (admitted database updates, batching and terminal invalidation; product sources own auth/recovery), `ErrorService` +
`getErrorMessage` / `isNetworkError`, **`TaskService.run`** (the async-action wrapper), **`DialogService`**
(responsive + i18n prompt/confirm/alert/component), `CopyService` (clipboard + toast),
`DownloadHTMLService`, `Exver` (version algebra façade), `SetupLogsService` +
`provideSetupLogsService(Api)`, `i18nService`/`i18nPipe`/`LocalizePipe` + `I18N_PROVIDERS`,
`MarkdownComponent`/`MARKDOWN` + `PromptModal`/`PROMPT` (ready `PolymorpheusComponent` dialogs),
`InitializingComponent`/`LogsWindowComponent`, `CaWizard` (+`CA_TRUST_CHECK`),
`DocsLinkDirective` (+`VERSION`, optional),
`IconDirective` (abstract base for an async icon `<img>`: subclass supplies `src`, null until
resolved; it owns the fallback and exposes `loading` until the image loads or fails for the
avatar's `[tuiSkeleton]`; `FALLBACK_ICON`),
`SafeLinksDirective`, pipes (`convertBytes`, `empty`, `compareExver`, `leafProgress`,
`markdown`), `RELATIVE_URL` token, `HttpError`/`RpcError`, disk/RPC/http types,
utils (`convertAnsi`, `formatProgress`, `getPkgId`, `pauseFor`, `withAbortSignal`, `@debounce`, `sameUrl`,
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
result types from method literals. `RpcStreamClient` / `createRpcStreamClient` infer
initial, item and final types from the generated tree; `RpcStreamOptions.signal`
cancels pending initialization as well as iteration. `RpcSocketService.stream` adapts
the existing encrypted session to that transport and retains initiating metadata. Use input declarations for requests; output
DTOs may contain required nullable fields absent from inputs. **Never hand-edit
`osBindings/*.ts`** — change the owning Rust schema and run `make start-core-ts-bindings`,
then rebuild core and SDK before checking consumers.

StartWRT uses its own generated `Api` in `services/api/bindings.ts` and streaming
schemas in `events.ts`; run `make start-wrt-rpc-bindings`. It composes shared signed-DH
RPC/DOM continuations, including unpersisted bootstrap admission, while retaining deadlines and its own reconnect owners. Shared `HttpService` remains a JSON-RPC transport boundary; product
wrappers infer results rather than accepting caller-selected return types.

Library authoring (when you add to `shared`/`marketplace`): configurability layers in order —
signal inputs → content projection/`contentChild(TemplateRef)`/`PolymorpheusContent` → abstract
class as DI contract → optional hook tokens → `provide*` factories → Taiga option providers.
Style with `--tui-*` vars, `:host { display: contents }` for a component composing several
siblings (one wrapping a single Taiga primitive becomes it via `hostDirectives`); no theme
definitions inside components.
