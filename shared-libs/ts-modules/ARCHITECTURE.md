# Web Architecture

Angular + TypeScript workspace using [Taiga UI](https://taiga-ui.dev/) component library.

## Place in the monorepo

This directory (`shared-libs/ts-modules`) holds the shared **libraries**; the Angular workspace root config (`angular.json`, `package.json`, `tsconfig.json`) lives at the repo root. The libraries live here directly:

- `shared/src/` — `@start9labs/shared` (Angular)
- `marketplace/src/` — `@start9labs/marketplace` (Angular)
- `start-core/lib/` — `@start9labs/start-core` (non-Angular; the SDK's core types/ABI/effects/OS bindings, consumed by web and bundled into the SDK. Has its own `Makefile`/`package.json`, built separately from the Angular workspace.)

The five **apps** are declared in `angular.json` but rooted in their product dirs, so paths below prefixed `ui/`, `setup-wizard/`, etc. actually live outside this directory:

| App                    | `angular.json` root                        |
| ---------------------- | ------------------------------------------ |
| `ui`                   | `../../projects/start-os/web/ui`           |
| `setup-wizard`         | `../../projects/start-os/web/setup-wizard` |
| `start-tunnel`         | `../../projects/start-tunnel/web`          |
| `start-wrt`            | `../../projects/start-wrt/web`             |
| `brochure-marketplace` | `../../projects/brochure-marketplace`      |

Apps import the libs via the root `tsconfig.json` path aliases `@start9labs/shared` → `./shared-libs/ts-modules/shared/src/public-api` and `@start9labs/marketplace` → `./shared-libs/ts-modules/marketplace/index`.

## API Layer (JSON-RPC)

Backend commands use JSON-RPC; prepared transfers and static resources retain HTTP.

The non-Angular core module owns `createRpcClient` / `RpcClient` for ordinary calls and
`createRpcStreamClient` / `RpcStreamClient` for typed response streams. Both compose the same
method-path proxy; neither imports a runtime schema tree or manages connections. A stream
transport returns the initiating result plus an `AsyncGenerator<Item, Final>`. Generated
`RpcIsStreaming`, `RpcStreamItemType` and `RpcStreamFinalType` select toolkit-owned endpoint
facts; non-streaming methods cannot be called on the stream client. Iterator disposal belongs
to its transport and must cancel only that operation, without replaying it or releasing siblings.

- **`HttpService`** (`shared/src/services/http.service.ts`) — HTTP transport. `rpcRequest()` prepares an unsigned request; `rpcMessage()` sends an already serialized message unchanged. Both use the shared JSON headers and response-compatibility adapter.
- **`AuthKeyService`** (`shared/src/services/auth-key.service.ts`) — Owns the browser's enrolled, nonextractable device key and its cancellation lifetime. `admissionLease` reuses possession of that key or an opaque, unpersisted Ed25519 signer when none is stored, without enrollment or a WebCrypto dependency for startup. Acquisition and enrollment started under a replaced, rolled-back or logged-out lifetime cannot adopt the next identity; fresh requests receive a fresh lifetime. StartOS and Tunnel use leases for encrypted RPC. StartWRT composes the same encrypted transport through its own RPC/reconnect owners; anonymous bootstrap uses unpersisted encrypted possession and omits normal-product peer-history observation. Resource requests keep header signing.
- **`RpcSocketService`** (`shared/src/services/rpc-socket.service.ts`) — StartOS/Tunnel/WRT's authenticated RPC and continuation transport. Its per-session abort primitive lets an existing recovery owner discard a wedged multiplexed socket without invalidating the enrolled key; DOM-style continuation sockets retain caller event ownership. Active RPC calls share an RxJS WebSocket subject; its last unsubscribe closes the socket. A two-message signed-DH exchange authenticates the selected hostname and logical RPC/GUID target before application traffic; each connection prepares a fresh hello before network deadlines, without per-message proofs or plaintext fallback. StartOS startup/bootstrap uses possession admission without requiring a stored key. WRT anonymous RPC uses the same encrypted owner without enrollment; its legacy HTTP endpoints remain for compatibility. Request serialization precedes asynchronous identity lookup; JSON response metadata remains authoritative. The bounded `ReadableStream` adapter retains physical framing/backpressure/closure ownership; Maintained Noble libraries supply X25519, HKDF-SHA256 and AES-256-GCM in one implementation shared by HTTP and HTTPS; browser randomness uses `getRandomValues`. Persisted enrollment keys remain nonextractable WebCrypto keys. The existing UUID library keeps request correlation available without secure-context `randomUUID`. The SDK auth module owns opaque signing providers and generic proof/key verification. Its asynchronous CBOR admission decoder validates wire kinds through the existing library's public decoder events before schema interpretation; it retains exact captured byte boundaries, rejects normalized tag/array/indefinite-byte alternatives, and destroys its bounded decoder after the first object or failure. Rust generates the hello schemas and transport/signature profiles. `rpc-stream.ts` owns each typed response stream's serialized UTF-8 byte-bounded queue and reserved terminal result; `RpcSocketService.stream` composes it with the existing correlation source. AbortSignals cancel pending initialization and iteration, including queued items after a terminal; iterator disposal also cancels before its first `next`. Application terminals drain accepted items, while transport loss discards them. Database subscriptions use the generated `RpcStreamClient`, deliver their initial dump without awaiting its own cache fence, and retain the source's existing batching/recovery lifetime.
- **`local-rpc.ts`** — `localRpcRequest` owns OS/Tunnel local capture, nullable-key encrypted-or-public-HTTP dispatch, error metadata and PatchDB fences. `authenticatedRpcTransport` owns device-key typed-stream admission and the combined device/operation signal; it rejects missing keys without HTTP fallback. Server authorization remains live. StartOS's normal generated stream facade waits for initial fences, shares its code-34 logout owner and preserves the socket iterator's early disposal. `watchDb$` composes the raw transport separately: its initial dump establishes the fence. Fence cancellation releases cache subscriptions and prevents late adoption after logout/disposal. Explicit foreign echo remains unsigned HTTP and unfenced; its rejection never invalidates local authentication.
- **`ServerIdentityService`** (`shared/src/services/server-identity.service.ts`) — Normal product API callers attach its open observer to RPC/stream/GUID operations. The transport caches one verified open event; reuse observes it only while its physical socket remains open. Observation precedes application replies, including authorization rejection, without introducing another connection owner. Same-origin API targets remember one fingerprint per app/API namespace, atomically in the existing identity IndexedDB store through `identity-store.ts`. This record is separate from the device-key record and survives logout. First contact is silent; rotation remembers the new key and lazily opens a nonblocking Taiga notice/details view. Storage failure never changes the RPC result. RPC, DOM and RxJS continuation open observers retain their owners. Startup, foreign API overrides and OS registry listings do not populate this history.
- **`StartupApiService`** (`ui/src/app/services/api/startup-api.service.ts`) — Explicit encrypted-possession boundary for StartOS initialization, diagnostics and the existing state/DNS probe. Setup-wizard calls and GUIDs use the same admission lease and socket owner. These callers neither enroll keys nor attach normal-product identity-history observers. Normal callers retain `ApiService` and its enrolled-key policy. An unauthenticated mode response never selects plaintext for a normal call. Diagnostic update observation uses the generated typed stream; the component-scoped `DiagnosticUpdate` service reports success after the actual terminal, not a progress counter or close code. Disposal cancels observation without canceling accepted backend work. Mock mode aliases the same concrete API fixture for both interfaces rather than duplicating its state; its initiating update and progress producers are shared with ordinary mock calls. Shared `pauseFor` accepts cancellation for mock progress waits. The setup wizard's component-scoped `SetupObservation` consumes typed `setup.status` through the same lease/socket owner; only successful termination permits navigation to its separate finalization step. Transport loss retries the read-only observation, never the accepted setup operation. Shared `isNetworkError` owns numeric code-zero classification for formatting and reconnect callers; StartWRT retains its existing exported binding and reconnect policy. Both transports share `rpcWebSocketUrl` and the native-generated endpoint.
- **`ApiService`** (`ui/src/app/services/api/embassy-api.service.ts`) — Abstract contract over the generated typed RPC client and product-specific operations. Two implementations:
  - `LiveApiService` — Production, calls the real backend
  - `MockApiService` — Development with mocks
- **`api.types.ts`** (`ui/src/app/services/api/api.types.ts`) — `ApiClient`, `Params`, and result aliases selected from the generated `RPC.StartOS` method tree.

**Calling an RPC endpoint from a component:**

```typescript
private readonly api = inject(ApiService)

async doSomething() {
  await this.api.rpc.echo({ message: 'ping' })
}
```

The live API reads `meta.patchSequence` — after a mutating call, it waits for the PatchDB WebSocket to catch up before resolving. The shared HTTP compatibility adapter projects older servers' `x-patch-sequence` headers when JSON metadata is absent. This ensures the UI always reflects the result of the call.

## PatchDB (Reactive State)

The backend pushes state diffs to the frontend via WebSocket. This is the primary way components get data.

- **`PatchDbSource`** (OS and Tunnel `services/patch-db/patch-db-source.ts`) — Authenticates before subscribing. Shared `databaseSource` delivers dumps immediately, batches revisions at 250ms and treats subscription termination as recovery, including successful terminals. OS requests a two-second state refresh and waits for the next running state; Tunnel retries after 500ms. Stopping the PatchDB consumer cancels initialization, iteration and recovery. API/auth owners handle live authorization rejection; product sources retain their separate retry policies.
- **`DataModel`** (`ui/src/app/services/patch-db/data-model.ts`) — TypeScript type for the full database shape (`ui`, `serverInfo`, `packageData`, `registryCache`, `osUpdateCache`).
- **`PatchDB<DataModel>`** — Injected service. Use `watch$()` to observe specific paths.

**Watching data in a component:**

```typescript
private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

// Watch a specific path — returns Observable, convert to Signal with toSignal()
readonly registry = toSignal(this.patch.watch$('serverInfo', 'osRegistry'))
readonly status = toSignal(this.patch.watch$('serverInfo', 'statusInfo'))
readonly packages = toSignal(this.patch.watch$('packageData'))
```

**In templates:** `{{ name() }}` — signals are called as functions.

## Stream consumers

1. **PatchDB** — Generated typed `db.subscribe`, managed by `PatchDbSource`; ordinary GUID replies remain compatible.
2. **Logs** — Portal and init/setup use generated typed follow operations; `LogsPipe` owns portal status/history and `SetupLogsService` owns startup accumulation. Portal batches at 1s, startup at 500ms. Shared `withAbortSignal` owns acquisition/iteration disposal for logs and database observations; each existing source retains its own read-only retry policy. Successful portal termination invalidates its ongoing observation and enters availability recovery. Ordinary/typed mock adapters share initiating and entry producers. Diagnostic historical logs remain finite RPC reads.
3. **Metrics** — Typed `server.metrics.follow` returns its shared sensor-cache snapshot, same-ID Metrics items and unit terminal; ordinary GUID replies remain compatible. `MetricsService` shares observation while authenticated consumers remain. Final unsubscribe/logout aborts acquisition and iteration and releases recovery; fresh observation/login gets a fresh snapshot. Network errors retain ConnectionService recovery; normal termination retains read-only resubscription. Mock ordinary/typed adapters share their snapshot/item producers.

## Navigation & Routing

- **Main app** (`ui/src/app/app.routes.ts`) — Route-based configuration with guards (`AuthGuard`, `UnauthGuard`, `stateNot()`), lazy loading via `loadChildren`, `PreloadAllModules`.
- **Portal routes** (`ui/src/app/routes/portal/portal.routes.ts`) — Modern array-based routes with `loadChildren` and `loadComponent`.
- **Setup wizard** (`setup-wizard/src/app/app.routes.ts`) — Standalone `loadComponent()` per step.
- Route config uses `bindToComponentInputs: true` — route params bind directly to component inputs.

## Forms

Two patterns:

1. **Dynamic (spec-driven)** — `FormService` (`ui/src/app/services/form.service.ts`) generates `FormGroup` from IST (Input Specification Type) schemas. Supports text, textarea, number, color, datetime, object, list, union, toggle, select, multiselect, file. Used for service configuration forms.

2. **Manual** — Standard Angular `FormGroup`/`FormControl` with validators. Used for login, setup wizard, system settings.

Form controls live in `ui/src/app/routes/portal/components/form/controls/` — each extends a base `Control<Spec, Value>` class and uses Taiga input components.

**Dialog-based forms** use `PolymorpheusComponent` + `TuiDialogContext` for modal rendering.

## i18n

- **`i18nPipe`** (`shared/src/i18n/i18n.pipe.ts`) — Translates English keys to the active language.
- **Dictionaries** live in `shared/src/i18n/dictionaries/` (en, es, de, fr, pl).
- Usage in templates: `{{ 'Some English Text' | i18n }}`

### How dictionaries work

- **`en.ts`** is the source of truth. Keys are English strings; values are numeric IDs (e.g. `'Domain Health': 748`).
- **Other language files** (`de.ts`, `es.ts`, `fr.ts`, `pl.ts`) use those same numeric IDs as keys, mapping to translated strings (e.g. `748: 'Santé du domaine'`).
- When adding a new i18n key:
  1. Add the English string and next available numeric ID to `en.ts`.
  2. Add the same numeric ID with a proper translation to every other language file.
  3. Always provide real translations, not empty strings.

## Services & State

Services often extend `Observable` and expose reactive streams via DI:

- **`ConnectionService`** — Combines network status + WebSocket readiness
- **`StateService`** — Polls server availability, manages app state (`running`, `initializing`, etc.)
- **`AuthService`** — Tracks `isVerified$`, triggers PatchDB start/stop
- **`PatchMonitorService`** — Starts/stops PatchDB based on auth state
- **`PatchDataService`** — Watches entire DB, updates localStorage bootstrap

## Frontend conventions

The root `start9-frontend` skill owns component, state, form, styling, and i18n
conventions. Read it before frontend work; these architecture notes describe
project-specific mechanisms, not a parallel style guide.

## Further reading

- [README.md](README.md) — what this is and how to use it
- [AGENTS.md](AGENTS.md) — setup, build/test/format, translations
