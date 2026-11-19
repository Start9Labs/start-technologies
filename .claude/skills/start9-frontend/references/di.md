# Dependency injection

- **`inject()` only.** Field initializers, chained freely
  (`private readonly params = toSignal(inject(ActivatedRoute).queryParamMap)`), inlined for
  single use (`inject(ActivatedRoute).snapshot.queryParamMap.get('code') || ''` directly inside
  a form-group literal — don't name a value used once).
- **`tuiProvide(TOKEN, Class)` for every `useExisting` binding** (type-safe):
  `tuiProvide(AbstractMarketplaceService, MarketplaceService)`,
  `tuiProvide(TuiLanguageSwitcherService, forwardRef(() => i18nService))`.
- **Root and node injectors only.** Singletons in `app.config.ts` or `@Service()`/
  `@Injectable({providedIn: 'root'})`; everything scoped goes in a **component's** `providers`/
  `viewProviders` — per-page service bindings (`providers: [provideFormService(WifiService)]`),
  per-dialog validation messages, per-subtree Taiga options. Never on a route (routing.md).
- **Tokens**: `new InjectionToken('description', { factory })` — and the factory often returns
  live state: a signal (`HELP_OPEN`), a `toSignal` of composed streams (`STATUS`), a
  `BehaviorSubject` (`PATCH_CACHE`). Consumers just `inject(STATUS)` and call it. Value tokens
  (`RELATIVE_URL`, `GIT_HASH`) bind in config. Abstract classes double as tokens for app-swapped
  implementations (`AbstractMarketplaceService`, `ApiService`); optional hook tokens use
  `inject(TOKEN, { optional: true })` so libs work without the app hook.
- **Provider factories** are exported next to the thing they provide:
  `provideFormService(Impl)`, `provideSetupLogsService(ApiClass)`,
  `provideTranslatedValidationErrors({...})` — apps compose providers, they don't hand-write
  `{ provide, useClass }` objects.
- **Browser globals via DI**: `WA_WINDOW`, `WA_LOCAL_STORAGE`, `WA_IS_MOBILE`,
  `WA_INTERSECTION_ROOT`, `inject(DOCUMENT)` (from `@angular/core`), `tuiInjectElement()` for
  the host element. Raw `window`/`document`/`localStorage` is tolerated only in
  infrastructure that genuinely means _the_ window (`ConnectionService` reload/online events).
- New services use **`@Service()`** (from `@angular/core`); `@Injectable({providedIn: 'root'})`
  is the older equivalent still present. Pipes that services also call imperatively are
  `@Pipe({...}) @Injectable({providedIn: 'root'})` and get injected (`inject(i18nPipe)`).
