import assert from 'node:assert/strict'
import { registerHooks } from 'node:module'
import test from 'node:test'
import '@angular/compiler'
import { BehaviorSubject, Subject } from 'rxjs'
import '../../../../shared-libs/ts-modules/shared/scripts/typescript-loader.mjs'

const shared = new URL(
  '../../../../shared-libs/ts-modules/shared/src/',
  import.meta.url,
)
const live = new URL(
  '../src/app/services/api/live-api.service.ts',
  import.meta.url,
)
const boundaries = new Map([
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
    export { localRpcRequest } from '${new URL('services/local-rpc.ts', shared)}';
    export { databaseSubscription, legacyDatabaseSubscription, bufferDatabaseRevisions } from '${new URL('services/database-subscription.ts', shared)}';
  `,
  ],
  ['../auth.service', 'export class AuthService {}'],
  ['../patch-db/patch-db-source', 'export const PATCH_CACHE = Symbol()'],
  ['./api.service', 'export class ApiService {}'],
])
registerHooks({
  resolve(specifier, context, next) {
    const source = context.parentURL === live.href && boundaries.get(specifier)
    if (!source) return next(specifier, context)
    return {
      url: `data:text/javascript,${encodeURIComponent(source)}`,
      shortCircuit: true,
    }
  },
})
const { LiveApiService } = await import(live.href)

function api() {
  const lease = { key: {}, signal: new AbortController().signal }
  const instance = {
    authKeys: { lease: async () => lease },
    identity: { openObserver: { next() {} } },
    sockets: { message: async () => ({ result: 'encrypted' }) },
    http: {
      rpcMessage: () => {
        throw new Error('unexpected plaintext request')
      },
    },
    cache$: new BehaviorSubject({ id: 0, value: {} }),
    auth: {
      deauthenticate() {
        instance.rejected++
      },
    },
    rejected: 0,
  }
  Object.setPrototypeOf(instance, LiveApiService.prototype)
  return {
    instance,
    request: options =>
      LiveApiService.prototype.rpcRequest.call(instance, options),
  }
}

test('encrypted replies still wait for their PatchDB sequence before returning', async () => {
  const { instance, request } = api()
  instance.cache$.next({ id: 1, value: {} })
  instance.sockets.message = async () => ({
    result: 'updated',
    meta: { patchSequence: 4 },
  })
  let completed = false
  const pending = request({ method: 'db.put', params: {} }).then(value => {
    completed = true
    return value
  })
  await Promise.resolve()
  await Promise.resolve()
  assert.equal(completed, false)
  instance.cache$.next({ id: 3, value: {} })
  await Promise.resolve()
  assert.equal(completed, false)
  instance.cache$.next({ id: 4, value: {} })
  assert.equal(await pending, 'updated')
  assert.equal(instance.rejected, 0)
})

test('database stream initialization establishes the cache fence and terminal authorization errors deauthenticate', async () => {
  const { instance } = api()
  const messages = new Subject()
  const received = []
  let failure
  const { rpcResponseStream } = await import(
    new URL('services/rpc-stream.ts', shared)
  )
  instance.sockets.stream = (body, lease, options) => {
    assert.deepEqual(JSON.parse(body), { method: 'db.subscribe', params: {} })
    assert.ok(lease.key)
    return rpcResponseStream(messages, options)
  }
  const subscription = instance.watchDb$().subscribe({
    next: value => received.push(value),
    error: error => {
      failure = error
    },
  })
  await new Promise(setImmediate)
  const dump = { id: 10, value: {} }
  messages.next({
    value: { jsonrpc: '2.0', result: dump, meta: { patchSequence: 999 } },
    bytes: 80,
  })
  await new Promise(setImmediate)
  assert.deepEqual(received, [dump])
  assert.equal(instance.cache$.value.id, 0)
  messages.next({
    value: {
      rpcstream: '1.0',
      end: true,
      error: { code: 34, message: 'revoked' },
    },
    bytes: 80,
  })
  await new Promise(setImmediate)
  assert.equal(failure.code, 34)
  assert.equal(instance.rejected, 1)
  assert.equal(subscription.closed, true)
  assert.equal(messages.observers.length, 0)
})

test('live authorization rejection still deauthenticates and preserves the RPC error', async () => {
  const { instance, request } = api()
  instance.sockets.message = async () => ({
    error: { code: 34, message: 'revoked', data: null },
  })
  await assert.rejects(
    request({ method: 'db.subscribe', params: {} }),
    error => error.code === 34 && error.message === 'RPC ERROR: revoked',
  )
  assert.equal(instance.rejected, 1)
})

test('absence of a device key preserves public HTTP bootstrap without encrypted fallback', async () => {
  const { instance, request } = api()
  instance.authKeys.lease = async () => null
  instance.sockets.message = () => {
    throw new Error('unexpected encrypted request')
  }
  const options = { method: 'auth.get-pubkey', params: {} }
  instance.http.rpcMessage = async (body, received) => {
    assert.equal(body, JSON.stringify(options))
    assert.equal(received, options)
    return { body: { result: 'public' } }
  }
  assert.equal(await request(options), 'public')
  assert.equal(instance.rejected, 0)
})

test('RPC parameters are captured before waiting for the device identity', async () => {
  const { instance, request } = api()
  let identity
  instance.authKeys.lease = () =>
    new Promise(resolve => {
      identity = resolve
    })
  instance.sockets.message = async body => {
    assert.deepEqual(JSON.parse(body), {
      method: 'echo',
      params: { value: 'original' },
    })
    return { result: 'captured' }
  }
  const params = { value: 'original' }
  const pending = request({ method: 'echo', params })
  params.value = 'changed'
  identity({ key: {}, signal: new AbortController().signal })
  assert.equal(await pending, 'captured')
})

test('GUID subscription ownership remains with the shared WebSocket subject', () => {
  const { instance } = api()
  const subject = new Subject()
  instance.sockets.open = guid => {
    assert.equal(guid, 'same-peer-guid')
    return subject
  }
  assert.equal(
    LiveApiService.prototype.openWebsocket$.call(instance, 'same-peer-guid'),
    subject,
  )
})
