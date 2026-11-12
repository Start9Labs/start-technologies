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

async function fixture(t, product) {
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
  const { PatchDbSource, PATCH_CACHE } = await import(
    path('patch-db/patch-db-source')
  )
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
      assert.deepEqual(JSON.parse(body), { method: 'db.subscribe', params: {} })
      assert.equal(options.openObserver, identity.openObserver)
      const frames = new Subject()
      const signal = AbortSignal.any([options.signal, lease.signal])
      calls.push({ frames, signal })
      return rpcResponseStream(frames, { ...options, signal })
    },
  }
  const injector = createEnvironmentInjector([
    { provide: ɵINJECTOR_SCOPE, useValue: 'root' },
    { provide: ɵChangeDetectionScheduler, useValue: { notify() {} } },
    { provide: DOCUMENT, useValue: { defaultView: {} } },
    { provide: NgZone, useValue: { run: task => task() } },
    { provide: Router, useValue: { navigate: async () => true } },
    {
      provide: StorageService,
      useValue: {
        get: () => ({ serverInfo: { hostname: 'cached' } }),
        clear() {},
      },
    },
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
  const dump = async (index, id) => {
    frame(index, {
      jsonrpc: '2.0',
      result: { id, value: { serverInfo: { hostname: `dump-${id}` } } },
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
    source,
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

for (const product of ['os', 'tunnel']) {
  test(`${product} production source applies immediate dumps and 250ms revisions through real PatchDB`, async t => {
    const f = await fixture(t, product)
    assert.equal(f.calls.length, 0)
    if (product === 'os') {
      assert.equal(f.cache.value.id, 1)
      assert.deepEqual(f.cache.value.value.registryCache, {})
      assert.equal(f.cache.value.value.osUpdateCache, null)
    } else assert.equal(f.cache.value.id, 0)
    f.verify()
    await flush()
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
