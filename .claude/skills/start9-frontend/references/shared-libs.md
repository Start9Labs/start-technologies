# Shared libraries — @start9labs/\*

`@start9labs/shared` (monorepo apps): `HttpService` (JSON-RPC), `ErrorService` +
`getErrorMessage`, **`TaskService.run`** (the async-action wrapper), **`DialogService`**
(responsive + i18n prompt/confirm/alert/component), `CopyService` (clipboard + toast),
`DownloadHTMLService`, `Exver` (version algebra façade), `SetupLogsService` +
`provideSetupLogsService(Api)`, `i18nService`/`i18nPipe`/`LocalizePipe` + `I18N_PROVIDERS`,
`MarkdownComponent`/`MARKDOWN` + `PromptModal`/`PROMPT` (ready `PolymorpheusComponent` dialogs),
`InitializingComponent`/`LogsWindowComponent`, `CaWizard` (+`CA_TRUST_CHECK`; the
http-origin Root CA trust walkthrough), `DocsLinkDirective` (+`VERSION`, optional),
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

`@start9labs/start-core`: generated OS types (`T.*` — the ts-rs projection of the Rust
backend), `IST`/`ISB` input-spec types/builders, `VersionRange`/`ExtendedVersion`, `S9pk`,
utils, zod re-export. **Never hand-edit `osBindings/*.ts`** — change the Rust type and run
`make start-core-ts-bindings`; a Rust `///` doc comment on an exported type changes the emitted
`.ts`.

Library authoring (when you add to `shared`/`marketplace`): configurability layers in order —
signal inputs → content projection/`contentChild(TemplateRef)`/`PolymorpheusContent` → abstract
class as DI contract → optional hook tokens → `provide*` factories → Taiga option providers.
Style with `--tui-*` vars, `:host { display: contents }` for a component composing several
siblings (one wrapping a single Taiga primitive becomes it via `hostDirectives`); no theme
definitions inside components.
