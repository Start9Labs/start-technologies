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
    Use `TaskService.runInline(task)` when the control already shows progress; it shares the
    error handling and success result without opening a blocking loader.
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

- **Monorepo = JSON-RPC**: StartOS/Tunnel's keyed local calls use `RpcSocketService`
  (`@start9labs/shared`) for signed-DH admission and encrypted messages. Each connection signs fresh RPC/GUID intent before the network deadline.
  Capture serialization before asynchronous key lookup; retain the existing RxJS socket and
  source-service retry/cancellation owners. `AuthKeyService` leases cancel on replacement,
  rollback and logout, including pending acquisition/enrollment. Public normal calls without a
  key and explicit foreign overrides retain HTTP; a failed exchange never retries as plaintext.
  StartOS's `StartupApiService` and setup-wizard use `AuthKeyService.admissionLease` and the same
  socket owner for initialization, diagnostics and state/DNS probes. Missing device keys use
  unpersisted possession without enrollment. Normal product APIs explicitly attach
  `ServerIdentityService.openObserver`; startup callers never populate persistent normal-server
  history. Per-operation open observation also covers healthy session reuse and authorization
  rejection. Callers select that contract explicitly; an unauthenticated mode response cannot
  downgrade a normal caller. Mock mode reuses the existing API fixture. RPC-level errors use
  `isRpcError(body)` → `RpcError`; local code-34 rejection forces logout; foreign probe rejection never invalidates local authentication. `localRpcRequest` owns OS/Tunnel local capture/dispatch, envelope errors and cancellable
  `meta.patchSequence` fences; foreign replies never wait on the local cache.
  `authenticatedRpcTransport` shares device-key stream admission with database observation; server authorization remains live.
  Normal generated streams wait for their initiating fences and preserve iterator disposal;
  identity replacement or caller disposal releases blocked fences. HTTP compatibility projects older `x-patch-sequence` headers only when JSON metadata
  is absent. Resource requests retain HTTP/header signatures. PatchDB uses the generated
  `RpcStreamClient` through the same socket owner: initial dump, typed revisions, terminal outcome.
  The initial dump establishes its own fence; never wait on that cache before delivering it.
  The source owns one AbortSignal through key lookup, initialization and iteration, and retains
  its existing 250ms batching/retry policy. Per-request queues bound serialized UTF-8 bytes,
  including backlogs held behind initial fences; exceeding the budget cancels that request. Terminal outcomes drain accepted items;
  physical loss discards them. Portal and startup logs consume typed items through their existing
  status/history/batching/retry owners; a successful portal terminal invalidates the ongoing view.
  Metrics uses generated typed follow with an initial snapshot and items from its existing sensor
  cache. AuthService gates ref-counted observation; final unsubscribe/logout cancels acquisition,
  iteration and recovery. Existing error recovery and normal-end read-only repeat remain.
- **start-wrt retains its RPC/reconnect workflow owners**: all browser RPC composes shared encrypted
  transport, with unpersisted anonymous admission before device enrollment. The caller omits
  normal-product peer-history observation for missing-key bootstrap. `admissionLease` carries
  the existing device key when present; avoid a second asynchronous lookup. Deadlines
  surface `{code: 0}`; `ConnectionService` and update recovery call the shared session-abort
  primitive before probing again. A timed-out multiplexed probe alone cannot release siblings
  holding a dead socket. Authenticated generated firmware metadata feeds the
  existing stale-UI detector, including metadata retained on initiating stream errors; the HTTP compatibility header remains for legacy clients. The wizard's component-scoped service consumes generated typed flash progress and requires successful termination before reporting completion. Its destruction cancels observation, never accepted flashing; network loss reuses ConnectionService without replay. DOM-style encrypted continuations preserve log/update event,
  close and retry ownership. Physical close starts recovery, not an update-success claim.
- **start9-store = REST + Zod through a BFF**: frontend calls `/api/*` only (SSR server proxies
  it — same relative URLs in dev/prod/SSR); domain types + Zod schemas live in `shared/`;
  commerce backends hide behind the API (`CommerceBackend` interface). The frontend must never
  know Shopify/Vendure exists. SSR cookie continuity via a 10-line functional interceptor
  cloning the incoming `REQUEST`'s cookie header.
- **Dashboards = REST `/_api`** on the same origin, no proxy/CORS/environments. All calls go
  through `AdminService` (authed; token signal from `localStorage`, `headers()` per request) or
  `ApiService` (public) — components never call `HttpClient` directly. Secrets stay server-side
  (the server proxies to sibling services adding API keys).
