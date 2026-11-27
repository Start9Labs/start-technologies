# State, data flow & backend integration

**Component boundary = signals. Streams compose once, in services.**

- Server data → template: `toSignal(this.store.watch$('serverInfo'))`,
  `toSignal(service.valueChanges)`, or `signal` set from `await firstValueFrom(api.call())`.
- HTTP reads in promise-land: `this.data.set(await firstValueFrom(this.api.list()))` inside
  `try/catch` with an `error` signal — template does the three-arm `@if`.
- **Service shapes**, pick by transport:
  - **Observable-subclass service** (push/composed state): `class ConnectionService extends
Observable<boolean>` with a private `stream$ = …pipe(shareReplay(1))` and
    `constructor() { super(subscriber => this.stream$.subscribe(subscriber)) }`. Consumers pipe
    the service itself: `inject(ConnectionService).pipe(filter(Boolean))`.
  - **Polling signal store** (a `FormService<T>`): `merge(load$, timer(0, 5000)).pipe(
switchMap(() => from(this.load()).pipe(catchError(...))), share())` → `data = toSignal(...)`;
    subclass implements `load()`/`store()`; page provides it with
    `provideFormService(Impl)` and reads one signal. Network errors collapse into the global
    reconnect toast — never per-poll error toasts.
  - **Push store**: the server pushes revisions over a websocket, batched (`bufferTime(250)`)
    into a client-side store; components `toSignal(store.watch$('key', 'path'))`.
  - **Server-owned state, replace-on-mutate** (a cart): every mutation returns the full object;
    `this.cart.set(await firstValueFrom(api.addLine(...)))`; `computed` for derived counts.
- **`.subscribe()` policy** — allowed shapes only: ① fire-and-forget completing streams
  (`alerts.open(...).subscribe()`, `dialogs.open(...).subscribe(result => …)` — dialog streams
  complete on close, and a dismissal is a completion with no value, so
  `defaultIfEmpty(null)` handles it in the same callback rather than a `complete:` handler and a
  flag); ② app-lifetime class-field subscriptions with `takeUntilDestroyed()`.
  Data for templates is never manually subscribed.
- **Async actions run through the app's action wrapper**, never per-component
  try/catch+loader. `FRONTEND.md` names the wrapper. Its shape: `run(task, 'Saving')` opens a
  `TuiNotificationMiddleService` loader, catches into the app's error service (negative toast,
  code-0 network mapping), and returns `Promise<boolean>` success. An inline variant, for a
  control that already shows progress, shares the error handling and success result without
  the blocking loader. A reconnect-aware app routes network drops into its reconnect indicator
  instead of a toast.
  - Errors classify as transport errors and application errors, shaped alike so the handler
    consumes either.
- **Loading UX**: `[tuiSkeleton]="!data()"` on tables/forms, `<tui-loader>` inline,
  `[loading]="saving()"` on buttons (`TuiButtonLoading`). No global spinners.
- No NgRx, no store library, ever.

## Backend integration

Every app follows **abstract `ApiService` + `LiveApiService` + `MockApiService`**, swapped at
DI level by build-time config or environment. Methods are **typed and promise-based**;
components call `await this.api.method(params)` and never touch HTTP.

- The app's transport, its error types and its dev/mock backend are in its `FRONTEND.md`,
  which links the `ARCHITECTURE.md` that owns them; read that before changing a transport,
  auth, data-source or reconnect owner.
- Components call the backend through the API service (or an authed wrapper of it), never
  `HttpClient` or the transport directly.
- Types come from the backend's generated bindings or shared schemas; never hand-edit generated
  files — regenerate them from their source.
- The frontend uses the same relative URLs in dev, prod and SSR; a same-origin server or BFF
  proxies, so there is no CORS and no per-environment URL. Under SSR, a functional interceptor
  copies the incoming `REQUEST`'s cookie header onto the server's API calls.
- Secrets stay server-side: the server proxies to third-party and sibling services and adds
  their keys. Behind a BFF, the frontend never learns which commerce or other backend serves it.
