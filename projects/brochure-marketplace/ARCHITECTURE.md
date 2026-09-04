# Architecture

`brochure` is the public Start9 Marketplace web app. It is a small Angular 22 application that wraps the shared `@start9labs/marketplace` component library and feeds it data from one or more package registries.

## Place in the monorepo

This is one application project in the single Angular workspace rooted at the repo root. It does not have its own `angular.json` or `package.json` — both live at the repo root, and `brochure-marketplace` is registered there as the `brochure-marketplace` project (root `angular.json`). Its `tsconfig.json` extends the repo-root `tsconfig.json` and maps the shared libs:

- `@start9labs/marketplace` → `shared-libs/ts-modules/marketplace`
- `@start9labs/shared` → `shared-libs/ts-modules/shared`
- `@start9labs/start-core` → built from `shared-libs/ts-modules/start-core/dist`

Because brochure renders the same `@start9labs/marketplace` library used by the in-OS `ui` app, the marketplace experience is identical whether viewed here or inside a server.

## Source layout (`projects/brochure-marketplace/src`)

```
src/
  main.ts                       bootstraps AppComponent with APP_CONFIG
  index.html                    document shell + Open Graph/Twitter meta
  styles.scss                   app-level styles (Taiga theme comes from shared)
  globals.d.ts                  ambient types
  environments/
    environment.ts              dev: production:false  -> MockApiService
    environment.prod.ts         prod: production:true  -> LiveApiService
  app/
    app.component.ts            <tui-root tuiTheme="dark"> + <router-outlet>
    app.config.ts              ApplicationConfig: router, HttpClient, Taiga, i18n, DI bindings
    app.routes.ts              '' -> MainComponent (lazy), '**' -> redirect ''
    components/
      main.component.ts        the marketplace view (registry picker, tiles, CTA)
    services/
      api.service.ts           abstract ApiService (the data contract)
      live-api.service.ts      real impl: RPC over HTTP to registries (prod)
      mock-api.service.ts      fixture impl backed by api.fixures.ts (dev)
      marketplace.service.ts   AbstractMarketplaceService impl (registry state)
      localize.ts              collapses LocaleString metadata to the active locale at ingress
      marketplace.service spec / fixtures / icons / md-sample.md
```

## Data flow

1. `app.config.ts` binds `ApiService` to `LiveApiService` (prod) or `MockApiService` (dev) based on `environment.production`, and binds `AbstractMarketplaceService` to `MarketplaceService` via `tuiProvide`.
2. `MainComponent` injects `MarketplaceService` and renders the shared `<marketplace>` component, passing the current registry plus two-way-bound `category` and `query` signals. It adds a "Get a Start9 server" CTA and the registry picker.
3. `MarketplaceService` (extends `AbstractMarketplaceService` from the shared lib) owns registry selection state:
   - Default registries (Start9 + Community) come from `@start9labs/shared`'s `defaultRegistries`.
   - Custom registries are persisted in `localStorage` under the `_startos/` prefix and exposed reactively via a `BehaviorSubject`.
   - It fetches registry info + packages through `ApiService` and converts raw registry responses into `MarketplacePkg` objects; fetch failures emit on `registryError$`.
   - **Localization boundary.** Registries return i18n metadata as `LocaleString` (`string | Record<lang, string>`) and only flatten it to one locale when a request carries a StartOS `device_info` — which this static, hostless site never sends. So `localize.ts` collapses every `LocaleString` field to the active locale here, as data enters the app, mirroring what the StartOS backend does for the embedded UI. Without it the raw `Record` form reaches templates and renders as `[object Object]`. This single boundary (rather than per-template `| localize`) is also where a future in-app language selector would re-localize.
4. `LiveApiService` talks to registries with JSON-RPC over HTTP (`RELATIVE_URL` = `/rpc/v0`), using `@start9labs/shared`'s `HttpService`; it also proxies static package files (`LICENSE.md`, `instructions.md`). `MockApiService` returns the fixtures instead.

## UI stack

Taiga UI 5 + the shared marketplace/shared libraries. The app is dark-themed (`tuiTheme="dark"`, `provideTaiga({ mode: 'dark' })`). User-facing strings go through the `i18n` pipe (`I18N_PROVIDERS`), with dictionaries living in `shared-libs/ts-modules/shared/src/i18n`.

## Build & deploy

`@angular/build:application` builds to `projects/brochure-marketplace/dist/raw/brochure-marketplace`. Assets are pulled from `shared/assets`, `projects/brochure-marketplace/src/assets/img`, and the Taiga icon set. Production builds apply the `environment.prod.ts` file replacement and output hashing. The deploy is automated on merge to `master` (`.github/workflows/deploy-brochure.yml`), which builds `@start9labs/start-core` + patch-db client first, then the brochure bundle, and ships it to the VPS hosting marketplace.start9.com.

## Further reading

- [README.md](README.md) — what this app is and how to run it.
- [CONTRIBUTING.md](CONTRIBUTING.md) — build/test/format workflow.
- [AGENTS.md](AGENTS.md) — instructions for AI/dev agents working here.
