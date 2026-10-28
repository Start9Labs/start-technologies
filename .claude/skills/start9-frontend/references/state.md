# State, data flow & backend integration

**Component boundary = signals. Streams compose once, in services.**

- Server data → template: `toSignal(this.patch.watch$('serverInfo'))`,
  `toSignal(service.valueChanges)`, or `signal` set from `await firstValueFrom(api.call())`.
- HTTP reads in promise-land: `this.data.set(await firstValueFrom(this.api.list()))` inside
  `try/catch` with an `error` signal — template does the three-arm `@if`.
- **Service shapes**, pick by transport:
  - **Observable-subclass service** (push/composed state): `class ConnectionService extends
Observable<boolean>` with a private `stream$ = …pipe(shareReplay(1))` and
    `constructor() { super(subscriber => this.stream$.subscribe(subscriber)) }`. Consumers pipe
    the service itself: `inject(ConnectionService).pipe(filter(Boolean))`.
  - **Polling signal store** (start-wrt `FormService<T>`): `merge(load$, timer(0, 5000)).pipe(
switchMap(() => from(this.load()).pipe(catchError(...))), share())` → `data = toSignal(...)`;
    subclass implements `load()`/`store()`; page provides it with
    `provideFormService(Impl)` and reads one signal. Network errors collapse into the global
    reconnect toast — never per-poll error toasts.
  - **PatchDB push** (StartOS/tunnel): websocket revisions → `bufferTime(250)` → `PatchDB`;
    components `toSignal(patch.watch$('key', 'path'))`.
  - **Server-owned state, replace-on-mutate** (store `CartService`): every mutation returns the
    full object; `this.cart.set(await firstValueFrom(api.addLine(...)))`; `computed` for
    derived counts.
- **`.subscribe()` policy** — allowed shapes only: ① fire-and-forget completing streams
  (`alerts.open(...).subscribe()`, `dialogs.open(...).subscribe(result => …)` — dialog streams
  complete on close, and a dismissal is a completion with no value, so
  `defaultIfEmpty(null)` handles it in the same callback rather than a `complete:` handler and a
  flag); ② app-lifetime class-field subscriptions with `takeUntilDestroyed()`.
  Data for templates is never manually subscribed.
- **Async actions run through the wrapper**, never per-component try/catch+loader:
  - Monorepo: `TaskService.run(task, 'Saving')` (`@start9labs/shared`) — opens a
    `TuiNotificationMiddleService` loader, catches into `ErrorService` (negative toast, code-0
    network mapping, optional support link), returns `Promise<boolean>` success.
  - start-wrt: `ActionService.run(action, { loading, success, fail, restart })` — same idea,
    reconnect-aware.
  - Errors classify as `HttpError` (transport) / `RpcError` (application), shaped alike so the
    handler consumes either.
- **Loading UX**: `[tuiSkeleton]="!data()"` on tables/forms, `<tui-loader>` inline,
  `[loading]="saving()"` on buttons (`TuiButtonLoading`). No global spinners.
- No NgRx, no store library, ever.

## Backend integration

Every app follows **abstract `ApiService` + `LiveApiService` + `MockApiService`**, swapped at
DI level by workspace config (monorepo) or environment (brochure). Methods are **typed and
promise-based**; components call `await this.api.method(params)` and never touch HTTP.

- **Monorepo = JSON-RPC** over `HttpService` (`@start9labs/shared`): POST `{ method, params }`
  to `RELATIVE_URL`, `withCredentials`; transport failures wrap as `HttpError`, RPC-level errors
  are checked with `isRpcError(body)` → `RpcError` (code 34 → force logout). StartOS's live API
  reads the `x-patch-sequence` header and awaits the PatchDB cache before resolving, so the UI
  always reflects a mutation's result. Streams (logs, PatchDB, metrics) via
  `api.openWebsocket$<T>(guid)` (`rxjs/webSocket`), buffered (`bufferTime(250…1000)`), with
  retry/backoff in the source service.
- **start-wrt keeps its own `HttpService`/`RpcService`/`ConnectionService` stack deliberately**:
  its per-request rxjs `timeout` **aborts the fetch on unsubscribe**, surfacing `{code: 0}`
  network errors that drive the reconnect UX. Don't swap it for shared's non-aborting client.
  (Requires `withFetch()`.)
- **start9-store = REST + Zod through a BFF**: frontend calls `/api/*` only (SSR server proxies
  it — same relative URLs in dev/prod/SSR); domain types + Zod schemas live in `shared/`;
  commerce backends hide behind the API (`CommerceBackend` interface). The frontend must never
  know Shopify/Vendure exists. SSR cookie continuity via a 10-line functional interceptor
  cloning the incoming `REQUEST`'s cookie header.
- **Dashboards = REST `/_api`** on the same origin, no proxy/CORS/environments. All calls go
  through `AdminService` (authed; token signal from `localStorage`, `headers()` per request) or
  `ApiService` (public) — components never call `HttpClient` directly. Secrets stay server-side
  (the server proxies to sibling services adding API keys).
