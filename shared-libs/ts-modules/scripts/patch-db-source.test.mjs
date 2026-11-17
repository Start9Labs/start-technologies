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
import { PatchDB } from 'patch-db-client'
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
const root = new URL('../../../projects/', import.meta.url)

async function fixture(
  t,
  product,
  { epoch, cachedValue = { serverInfo: { hostname: 'cached' } } } = {},
) {
  t.mock.timers.enable({ apis: ['setInterval', 'setTimeout'], now: Date.now() })
  const scope = product === 'os' ? 'start-os/web/ui' : 'start-tunnel/web'
  const path = file => new URL(`${scope}/src/app/services/${file}.ts`, root)
  const { AuthService } = await import(path('auth.service'))
  const { ApiService } = await import(
    path(product === 'os' ? 'api/embassy-api.service' : 'api/api.service')
  )
  const { LiveApiService } = await import(
    path(
      product === 'os'
        ? 'api/embassy-live-api.service'
        : 'api/live-api.service',
    )
  )
  const { PatchDbSource, PATCH_CACHE, PATCH_LIVE } = await import(
    path('patch-db/patch-db-source')
  )
  const patchCache = {
    epoch,
    read: async () => ({ id: 7, value: cachedValue }),
    clear() {},
  }
  const patchCacheProvider =
    product === 'os'
      ? [
          {
            provide: (await import(path('patch-db/patch-db-cache')))
              .PatchDbCache,
            useValue: patchCache,
          },
        ]
      : []
  const { Router } = await import('@angular/router')
  const cache = new BehaviorSubject({ id: 0, value: {} })
  const state = new BehaviorSubject('running')
  const retriggers = []
  state.retrigger = (...args) => retriggers.push(args)
  let lifetime = new AbortController(),
    pendingLease
  const calls = [],
    reads = [],
    clears = []
  const key = {
    key: {},
    get signal() {
      return lifetime.signal
    },
  }
  const keys = {
    get: async () => null,
    lease: () => {
      reads.push(true)
      return (
        pendingLease?.promise ??
        Promise.resolve({ key: key.key, signal: lifetime.signal })
      )
    },
    clear: () => {
      clears.push(true)
      lifetime.abort()
    },
  }
  const identity = { openObserver: { next() {} } }
  const sockets = {
    stream: (body, lease, options) => {
      const { method, params } = JSON.parse(body)
      assert.equal(method, 'db.subscribe')
      assert.equal(options.openObserver, identity.openObserver)
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
    ...patchCacheProvider,
    { provide: AuthKeyService, useValue: keys },
    { provide: HttpService, useValue: {} },
    { provide: RpcSocketService, useValue: sockets },
    { provide: ServerIdentityService, useValue: identity },
    { provide: StateService, useValue: state },
    { provide: PATCH_CACHE, useValue: cache },
    { provide: ApiService, useClass: LiveApiService },
    AuthService,
    PatchDbSource,
  ])
  const auth = injector.get(AuthService)
  const source = injector.get(PatchDbSource)
  const effects = injector.get(ɵEffectScheduler)
  const db = new PatchDB(source, cache)
  await auth.init()
  effects.flush()
  db.start()
  await flush()
  t.after(() => {
    db.stop()
    injector.destroy()
  })
  const verify = () => {
    lifetime = new AbortController()
    if (product === 'os') auth.setVerified()
    else auth.authenticated.set(true)
    effects.flush()
  }
  const logout = () => {
    if (product === 'os') auth.setUnverified(true)
    else auth.deauthenticate()
    effects.flush()
  }
  const frame = (index, value) => {
    const message = { id: `source-${index}`, ...value }
    calls[index].frames.next({
      value: message,
      bytes: Buffer.byteLength(JSON.stringify(message)),
    })
  }
  const dump = async (
    index,
    id,
    value = { serverInfo: { hostname: `dump-${id}` } },
  ) => {
    frame(index, {
      jsonrpc: '2.0',
      result: { id, value },
      meta: { patchSequence: 999 },
    })
    await flush()
  }
  const revision = (index, id) =>
    frame(index, {
      rpcstream: '1.0',
      end: false,
      result: {
        id,
        patch: [
          {
            op: 'replace',
            path: '/serverInfo/hostname',
            value: `revision-${id}`,
          },
        ],
      },
    })
  const recover = async () => {
    if (product === 'os') {
      state.next('error')
      state.next('running')
    } else t.mock.timers.tick(500)
    effects.flush()
    await flush()
  }
  return {
    db,
    patchCache,
    source,
    connection: product === 'os' ? injector.get(PATCH_LIVE) : undefined,
    auth,
    effects,
    cache,
    calls,
    reads,
    clears,
    retriggers,
    state,
    verify,
    logout,
    dump,
    revision,
    frame,
    recover,
    delayLease: () => {
      pendingLease = Promise.withResolvers()
      return pendingLease
    },
    currentLease: () => ({ key: key.key, signal: lifetime.signal }),
  }
}

for (const status of ['connected', 'error']) {
  test(`os registry status shows the last known observation from restore through a live ${status} dump and stream loss`, async t => {
    const url = 'https://registry.test/'
    const entry = {
      status: 'error',
      error: 'cached registry failure',
      info: { name: 'Cached registry' },
      packages: {
        example: { best: { '1.0.0:0': { title: 'Example' } } },
      },
    }
    const cachedValue = {
      serverInfo: { hostname: 'cached' },
      registryCache: { [url]: entry },
    }
    const f = await fixture(t, 'os', { cachedValue })
    const registries = []
    const confirmations = []
    const subscription = f.db
      .watch$('registryCache')
      .subscribe(cache => registries.push(cache[url]))
    const connection = f.connection.subscribe(connected => {
      if (connected) confirmations.push(f.cache.value.id)
    })
    t.after(() => {
      subscription.unsubscribe()
      connection.unsubscribe()
    })
    f.verify()
    await flush()
    assert.equal(f.state.value, 'running')
    assert.equal(f.calls.length, 1)
    assert.equal(f.connection.value, false)
    assert.deepEqual(registries.at(-1), entry)

    const live = {
      ...entry,
      status,
      error: status === 'error' ? 'live registry failure' : null,
    }
    const dump = { ...cachedValue, registryCache: { [url]: live } }
    await f.dump(0, 10, dump)
    assert.deepEqual(registries.at(-1), live)
    assert.equal(f.connection.value, true)
    assert.deepEqual(confirmations, [10])

    const emissions = registries.length
    f.calls[0].frames.error({ code: 0, message: 'server lost' })
    await flush()
    assert.equal(f.state.value, 'running')
    assert.equal(f.connection.value, false)
    assert.equal(registries.length, emissions)
    assert.deepEqual(registries.at(-1), live)
    await f.recover()
    assert.equal(f.calls.length, 2)
    assert.equal(f.connection.value, false)
    await f.dump(1, 20, dump)
    assert.equal(f.connection.value, true)
    assert.deepEqual(confirmations, [10, 20])
    f.logout()
    assert.equal(f.connection.value, false)
    f.verify()
    await flush()
    assert.equal(f.connection.value, false)
    await f.dump(2, 30, dump)
    assert.equal(f.connection.value, true)
    assert.deepEqual(confirmations, [10, 20, 30])
    f.db.stop()
    assert.equal(f.connection.value, false)
  })
}

for (const product of ['os', 'tunnel']) {
  test(`${product} production source applies immediate dumps and 250ms revisions through real PatchDB`, async t => {
    const f = await fixture(t, product)
    assert.equal(f.calls.length, 0)
    if (product === 'os') {
      assert.equal(f.cache.value.id, 7)
      assert.equal(f.cache.value.value.serverInfo.hostname, 'cached')
    } else assert.equal(f.cache.value.id, 0)
    f.verify()
    await flush()
    assert.deepEqual(f.calls[0].params, {})
    await f.dump(0, 10)
    assert.equal(f.cache.value.id, 10)
    f.revision(0, 11)
    await flush()
    assert.equal(f.cache.value.id, 10)
    t.mock.timers.tick(249)
    assert.equal(f.cache.value.id, 10)
    t.mock.timers.tick(1)
    assert.equal(f.cache.value.id, 11)
    assert.equal(f.cache.value.value.serverInfo.hostname, 'revision-11')
  })

  test(`${product} disposal during key lookup cannot dispatch or reopen a database stream`, async t => {
    const f = await fixture(t, product)
    const pending = f.delayLease()
    f.verify()
    await flush()
    assert.equal(f.reads.length, 1)
    f.db.stop()
    pending.resolve(f.currentLease())
    await flush()
    await f.recover()
    assert.equal(f.calls.length, 0)
    assert.equal(f.cache.value.id, 0)
  })

  test(`${product} stop during pending initialization rejects late replies without reopening`, async t => {
    const f = await fixture(t, product)
    f.verify()
    await flush()
    assert.equal(f.calls.length, 1)
    f.db.stop()
    await flush()
    assert.equal(f.calls[0].signal.aborted, true)
    assert.equal(f.calls[0].frames.observers.length, 0)
    await f.dump(0, 10)
    await f.recover()
    assert.equal(f.calls.length, 1)
    assert.equal(f.cache.value.id, 0)
  })

  test(`${product} a new authenticated source cannot inherit the abandoned pending key lookup`, async t => {
    const f = await fixture(t, product)
    const pending = f.delayLease()
    f.verify()
    await flush()
    f.logout()
    f.verify()
    await flush()
    assert.equal(f.reads.length, 2)
    pending.resolve(f.currentLease())
    await flush()
    assert.equal(f.calls.length, 1)
    await f.dump(0, 20)
    assert.equal(f.cache.value.id, 20)
  })

  test(`${product} loss discards pending revisions and recovers with a new dump, never the old stream`, async t => {
    const f = await fixture(t, product)
    f.verify()
    await flush()
    await f.dump(0, 10)
    f.revision(0, 11)
    await flush()
    f.calls[0].frames.error({ code: 0, message: 'lost' })
    await flush()
    assert.equal(f.calls[0].signal.aborted, true)
    assert.equal(f.calls[0].frames.observers.length, 0)
    assert.equal(f.cache.value.id, 10)
    if (product === 'os') {
      assert.deepEqual(f.retriggers, [[false, 2000]])
      f.state.next('error')
      t.mock.timers.tick(5000)
      await flush()
      assert.equal(f.calls.length, 1)
    } else {
      t.mock.timers.tick(499)
      await flush()
      assert.equal(f.calls.length, 1)
    }
    await f.recover()
    assert.equal(f.calls.length, 2)
    await f.dump(1, 20)
    f.revision(0, 11)
    await flush()
    t.mock.timers.tick(250)
    assert.equal(f.cache.value.id, 20)
  })

  test(`${product} successful subscription termination still recovers, while stop cancels recovery`, async t => {
    const f = await fixture(t, product)
    f.verify()
    await flush()
    await f.dump(0, 10)
    f.revision(0, 11)
    await flush()
    assert.equal(f.cache.value.id, 10)
    f.frame(0, { rpcstream: '1.0', result: null, end: true })
    await flush()
    assert.equal(f.cache.value.id, 11)
    assert.equal(f.calls[0].signal.aborted, true)
    await f.recover()
    assert.equal(f.calls.length, 2)
    f.calls[1].frames.error({ code: 0, message: 'lost again' })
    await flush()
    f.db.stop()
    await f.recover()
    assert.equal(f.calls.length, 2)
    assert.equal(f.cache.value.id, 0)
  })

  test(`${product} logout cancels initialization and active batching, and login uses a fresh stream`, async t => {
    const f = await fixture(t, product)
    f.verify()
    await flush()
    f.logout()
    await flush()
    assert.equal(f.calls[0].signal.aborted, true)
    await f.dump(0, 10)
    await f.recover()
    assert.equal(f.calls.length, 1)
    f.verify()
    await flush()
    assert.equal(f.calls.length, 2)
    await f.dump(1, 20)
    f.revision(1, 21)
    await flush()
    f.logout()
    await flush()
    t.mock.timers.tick(250)
    assert.equal(f.cache.value.id, 20)
    assert.equal(f.calls[1].signal.aborted, true)
  })

  for (const phase of ['initial', 'terminal']) {
    test(`${product} ${phase} authorization rejection uses the real authentication owner and cannot reconnect while logged out`, async t => {
      const f = await fixture(t, product)
      f.verify()
      await flush()
      if (phase === 'terminal') await f.dump(0, 10)
      const clearedBefore = f.clears.length
      f.frame(0, {
        ...(phase === 'initial'
          ? { jsonrpc: '2.0' }
          : { rpcstream: '1.0', end: true }),
        error: { code: 34, message: 'revoked' },
      })
      await flush()
      f.effects.flush()
      if (product === 'os') {
        const values = []
        f.auth.isVerified$.subscribe(value => values.push(value)).unsubscribe()
        assert.equal(values.at(-1), false)
      } else assert.equal(f.auth.authenticated(), false)
      await f.recover()
      assert.equal(f.calls.length, 1)
      assert.equal(f.clears.length, clearedBefore + 1)
    })
  }
}

test('os resumes from its snapshot, then from its live revision after loss', async t => {
  const f = await fixture(t, 'os', { epoch: 'boot-1' })
  f.verify()
  await flush()
  assert.deepEqual(f.calls[0].params, { since: { epoch: 'boot-1', id: 7 } })
  f.frame(0, {
    jsonrpc: '2.0',
    result: {
      epoch: 'boot-1',
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
    },
    meta: { patchSequence: 9 },
  })
  await flush()
  assert.equal(f.cache.value.id, 9)
  assert.equal(f.cache.value.value.serverInfo.hostname, 'resumed')
  f.calls[0].frames.error({ code: 0, message: 'lost' })
  await flush()
  await f.recover()
  assert.deepEqual(f.calls[1].params, { since: { epoch: 'boot-1', id: 9 } })
  f.frame(1, {
    jsonrpc: '2.0',
    result: {
      epoch: 'boot-2',
      id: 3,
      value: { serverInfo: { hostname: 'rebooted' } },
      revisions: null,
    },
    meta: { patchSequence: 3 },
  })
  await flush()
  assert.equal(f.patchCache.epoch, 'boot-2')
  assert.equal(f.cache.value.id, 3)
  assert.equal(f.cache.value.value.serverInfo.hostname, 'rebooted')
})
