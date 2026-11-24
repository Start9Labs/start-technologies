import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import {
  DOCUMENT,
  NgZone,
  createEnvironmentInjector,
  ɵINJECTOR_SCOPE,
  ɵEffectScheduler,
  ɵChangeDetectionScheduler,
} from '@angular/core'
import { BehaviorSubject, Subject } from 'rxjs'
import { loadTypeScript } from '../../../projects/start-os/web/ui/scripts/typescript-loader.mjs'

const shared = new URL('../shared/src/', import.meta.url)
const storage = 'data:text/javascript,export class StorageService {}'
loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      `
    export { AuthKeyService } from '${new URL('services/auth-key.service.ts', shared)}';
    export { HttpService } from '${new URL('services/http.service.ts', shared)}';
    export { RpcSocketService } from '${new URL('services/rpc-socket.service.ts', shared)}';
    export { ServerIdentityService } from '${new URL('services/server-identity.service.ts', shared)}';
    export { RpcError } from '${new URL('classes/rpc-error.ts', shared)}';
    export { isRpcError } from '${new URL('types/rpc.types.ts', shared)}';
    export { rpcRequestBody } from '${new URL('services/rpc-request.ts', shared)}';
    export { rpcResponseResult, waitForPatchSequence } from '${new URL('services/rpc-response.ts', shared)}';
    export { authenticatedRpcTransport, localRpcRequest } from '${new URL('services/local-rpc.ts', shared)}';
    export { databaseSource, databaseSubscription, legacyDatabaseSubscription } from '${new URL('services/database-subscription.ts', shared)}';
  `,
    ],
    ['./storage.service', `export { StorageService } from '${storage}'`],
    ['../storage.service', `export { StorageService } from '${storage}'`],
    ['src/app/services/state.service', 'export class StateService {}'],
  ]),
)
const { AuthKeyService, HttpService, RpcSocketService, ServerIdentityService } =
  await import('@start9labs/shared')
const { StorageService } = await import(storage)
const { StateService } = await import('src/app/services/state.service')
const { rpcResponseStream } = await import(
  new URL('services/rpc-stream.ts', shared)
)
const flush = () => new Promise(setImmediate)
const services = new URL(
  '../../../projects/start-os/web/ui/src/app/services/',
  import.meta.url,
)
const service = file => import(new URL(`${file}.ts`, services))

async function fixture(t, { snapshot } = {}) {
  t.mock.timers.enable({ apis: ['setInterval', 'setTimeout'], now: Date.now() })
  const { AuthService } = await service('auth.service')
  const { ApiService } = await service('api/embassy-api.service')
  const { LiveApiService } = await service('api/embassy-live-api.service')
  const { PatchDbCache } = await service('patch-db/patch-db-cache')
  const { PatchMonitorService } = await service('patch-monitor.service')
  const { PATCH_CACHE, PATCH_DB, PATCH_LIVE } = await service(
    'patch-db/patch-db-source',
  )
  const { Router } = await import('@angular/router')
  const empty = { dump: { id: 0, value: {} }, cursors: {} }
  const patchCache = {
    read: async () => structuredClone(snapshot ?? empty),
    clears: 0,
    clear() {
      this.clears++
    },
  }
  const state = new BehaviorSubject('running')
  const retriggers = []
  state.retrigger = (...args) => retriggers.push(args)
  let lifetime = new AbortController()
  const calls = []
  const keys = {
    get: async () => null,
    lease: () => Promise.resolve({ key: {}, signal: lifetime.signal }),
    clear: () => lifetime.abort(),
  }
  const identity = { openObserver: { next() {} } }
  const sockets = {
    stream: (body, lease, options) => {
      const { method, params } = JSON.parse(body)
      assert.equal(method, 'db.subscribe')
      const frames = new Subject()
      const signal = AbortSignal.any([options.signal, lease.signal])
      calls.push({ frames, signal, params })
      return rpcResponseStream(frames, { ...options, signal })
    },
  }
  const injector = createEnvironmentInjector([
    { provide: ɵINJECTOR_SCOPE, useValue: 'root' },
    { provide: ɵChangeDetectionScheduler, useValue: { notify() {} } },
    { provide: DOCUMENT, useValue: { defaultView: {} } },
    { provide: NgZone, useValue: { run: task => task() } },
    { provide: Router, useValue: { navigate: async () => true } },
    { provide: StorageService, useValue: { clear() {} } },
    { provide: PatchDbCache, useValue: patchCache },
    { provide: AuthKeyService, useValue: keys },
    { provide: HttpService, useValue: {} },
    { provide: RpcSocketService, useValue: sockets },
    { provide: ServerIdentityService, useValue: identity },
    { provide: StateService, useValue: state },
    { provide: ApiService, useClass: LiveApiService },
    AuthService,
    PatchMonitorService,
  ])
  const updates = []
  injector.get(ApiService).updateDbSubscription = async (
    subscription,
    selection,
  ) => {
    updates.push({ subscription, selection })
  }
  const auth = injector.get(AuthService)
  const effects = injector.get(ɵEffectScheduler)
  const db = injector.get(PATCH_DB)
  const cache = injector.get(PATCH_CACHE)
  const live = injector.get(PATCH_LIVE)
  const session = injector.get(PatchMonitorService).subscribe()
  await auth.init()
  effects.flush()
  t.after(() => {
    session.unsubscribe()
    injector.destroy()
  })
  const open = () => calls.filter(c => !c.signal.aborted)
  const frame = (call, value) => {
    const message = { id: `source-${calls.indexOf(call)}`, ...value }
    call.frames.next({
      value: message,
      bytes: Buffer.byteLength(JSON.stringify(message)),
    })
  }
  const start = async (result, call = open()[0]) => {
    frame(call, {
      jsonrpc: '2.0',
      result: { epoch: 'boot', subscription: 'sub', ...result },
      meta: { patchSequence: result.id },
    })
    await flush()
  }
  const item = async (result, call = open()[0]) => {
    frame(call, { rpcstream: '1.0', end: false, result })
    await flush()
  }
  const verify = async () => {
    lifetime = new AbortController()
    auth.setVerified()
    effects.flush()
    await flush()
  }
  return {
    auth,
    effects,
    db,
    cache,
    live,
    state,
    retriggers,
    patchCache,
    calls,
    updates,
    open,
    frame,
    start,
    item,
    verify,
  }
}

const ALWAYS = [
  { include: '/packageData' },
  { include: '/serverInfo' },
  { include: '/ui' },
]

test('os subscribes once to the always-watched paths and is live once that starts', async t => {
  const f = await fixture(t)
  assert.equal(f.calls.length, 0)
  await f.verify()
  assert.deepEqual(
    f.calls.map(c => c.params),
    [{ pointer: '/public', selectors: ALWAYS }],
  )
  assert.equal(f.live.value, false)
  await f.start({
    id: 10,
    value: { serverInfo: { hostname: 'box' }, packageData: {}, ui: {} },
  })
  assert.equal(f.live.value, true)
  assert.equal(f.cache.value.id, 10)
  assert.equal(f.cache.value.value.serverInfo.hostname, 'box')
  assert.deepEqual(f.updates, [])
})

test('os widens the subscription for a page without affecting the live state', async t => {
  const f = await fixture(t)
  await f.verify()
  await f.start({ id: 10, value: { serverInfo: {}, packageData: {}, ui: {} } })
  const registries = []
  const subscription = f.db
    .watch$('registryCache')
    .subscribe(value => registries.push(value))
  await flush()
  assert.deepEqual(f.updates, [
    {
      subscription: 'sub',
      selection: [
        { include: '/packageData' },
        { include: '/registryCache' },
        { include: '/serverInfo' },
        { include: '/ui' },
      ],
    },
  ])
  assert.equal(f.live.value, true)
  assert.deepEqual(registries, [])
  await f.item({
    id: 10,
    extend: true,
    patch: [{ op: 'add', path: '/registryCache', value: { r: {} } }],
  })
  assert.deepEqual(registries, [{ r: {} }])
  subscription.unsubscribe()
  t.mock.timers.tick(5000)
  await flush()
  assert.deepEqual(f.updates.at(-1).selection, ALWAYS)
  assert.equal(f.cache.value.value.registryCache, undefined)
  assert.equal(f.calls.length, 1)
})

test('os resumes its persisted selection from its cursor and recovers from loss', async t => {
  const cursor = { epoch: 'boot', id: 7 }
  const f = await fixture(t, {
    snapshot: {
      dump: { id: 7, value: { serverInfo: { hostname: 'cached' } } },
      cursors: { '/serverInfo': cursor },
    },
  })
  const hostnames = []
  f.db.watch$('serverInfo', 'hostname').subscribe(h => hostnames.push(h))
  await f.verify()
  assert.deepEqual(hostnames, ['cached'])
  assert.deepEqual(f.calls[0].params, {
    pointer: '/public',
    since: cursor,
    selectors: [{ include: '/serverInfo' }],
  })
  await f.start({
    id: 9,
    value: null,
    revisions: [
      {
        id: 8,
        patch: [
          { op: 'replace', path: '/serverInfo/hostname', value: 'resumed' },
        ],
      },
    ],
  })
  assert.deepEqual(hostnames.at(-1), 'resumed')
  assert.deepEqual(f.updates.at(-1).selection, ALWAYS)
  assert.equal(f.live.value, false)
  await f.item({
    id: 9,
    extend: true,
    patch: [
      { op: 'add', path: '/packageData', value: {} },
      { op: 'add', path: '/ui', value: {} },
    ],
  })
  assert.equal(f.live.value, true)

  f.open()[0].frames.error({ code: 0, message: 'lost' })
  await flush()
  assert.equal(f.live.value, false)
  assert.deepEqual(f.retriggers, [[false, 2000]])
  assert.equal(f.open().length, 0)
  f.state.next('error')
  f.state.next('running')
  await flush()
  assert.deepEqual(f.calls[1].params, {
    pointer: '/public',
    since: { epoch: 'boot', id: 9 },
    selectors: ALWAYS,
  })
})

test('os rejection of the session logs out, clears the browser cache and does not reconnect', async t => {
  const f = await fixture(t)
  await f.verify()
  await f.start({ id: 10, value: { serverInfo: {}, packageData: {}, ui: {} } })
  const clears = f.patchCache.clears
  f.frame(f.open()[0], {
    rpcstream: '1.0',
    end: true,
    error: { code: 34, message: 'revoked' },
  })
  await flush()
  f.effects.flush()
  const values = []
  f.auth.isVerified$.subscribe(v => values.push(v)).unsubscribe()
  assert.equal(values.at(-1), false)
  assert.equal(f.patchCache.clears, clears + 1)
  assert.equal(f.live.value, false)
  assert.deepEqual(f.cache.value, { id: 0, value: {} })
  f.state.next('error')
  f.state.next('running')
  await flush()
  assert.equal(f.open().length, 0)
})
