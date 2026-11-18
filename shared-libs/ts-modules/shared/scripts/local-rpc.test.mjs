import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { getEventListeners } from 'node:events'
import { BehaviorSubject } from 'rxjs'
import './typescript-loader.mjs'
const { localRpcRequest, authenticatedRpcTransport } =
  await import('../src/services/local-rpc.ts')
const { rpcResponseResult, waitForPatchSequence } =
  await import('../src/services/rpc-response.ts')
const flush = () => new Promise(setImmediate)
const identity = { openObserver: { next() {} } }
const lease = () => ({ key: {}, signal: new AbortController().signal })

test('normal local calls capture params before key lookup and retain their verified observer', async () => {
  const acquired = Promise.withResolvers(),
    key = lease(),
    params = { value: 'captured' }
  let captured
  const pending = localRpcRequest(
    {},
    {
      message: async (body, options, actual) => {
        captured = { body, options, actual }
        return { result: 'reply' }
      },
    },
    { lease: () => acquired.promise },
    identity,
    new BehaviorSubject({ id: 0 }),
    { method: 'example', params, headers: { retained: 'yes' } },
  )
  params.value = 'changed'
  acquired.resolve(key)
  assert.equal(await pending, 'reply')
  assert.equal(JSON.parse(captured.body).params.value, 'captured')
  assert.equal(captured.actual, key)
  assert.equal(captured.options.openObserver, identity.openObserver)
  assert.equal(captured.options.headers.retained, 'yes')
})

test('ordinary missing-key calls retain HTTP and wait for the local committed sequence', async () => {
  const cache = new BehaviorSubject({ id: 0 })
  let settled = false
  const pending = localRpcRequest(
    {
      rpcMessage: async () => ({
        body: { result: 'public', meta: { patchSequence: 2 } },
      }),
    },
    { message: () => assert.fail('not keyed') },
    { lease: async () => null },
    identity,
    cache,
    { method: 'echo', params: {} },
  ).then(value => {
    settled = true
    return value
  })
  await flush()
  assert.equal(settled, false)
  cache.next({ id: 2 })
  assert.equal(await pending, 'public')
  assert.equal(cache.observed, false)
})

test('normal identity invalidation cancels a blocked fence and releases both subscriptions', async () => {
  const abort = new AbortController(),
    cache = new BehaviorSubject({ id: 0 })
  const pending = localRpcRequest(
    {},
    { message: async () => ({ result: null, meta: { patchSequence: 999 } }) },
    { lease: async () => ({ key: {}, signal: abort.signal }) },
    identity,
    cache,
    { method: 'example', params: {} },
  )
  await flush()
  assert.equal(cache.observed, true)
  assert.equal(getEventListeners(abort.signal, 'abort').length, 1)
  abort.abort()
  await assert.rejects(pending, { name: 'AbortError' })
  assert.equal(cache.observed, false)
  assert.equal(getEventListeners(abort.signal, 'abort').length, 0)
})

test('identity replacement discards an already resolved old authorization rejection', async () => {
  const abort = new AbortController(),
    response = Promise.withResolvers()
  const pending = localRpcRequest(
    {},
    { message: () => response.promise },
    { lease: async () => ({ key: {}, signal: abort.signal }) },
    identity,
    new BehaviorSubject({ id: 0 }),
    { method: 'example', params: {} },
  )
  await flush()
  response.resolve({ error: { code: 34, message: 'old identity', data: null } })
  abort.abort()
  await assert.rejects(pending, { name: 'AbortError' })
})

test('RPC errors retain authenticated metadata and precede cache fences', async () => {
  const meta = { patchSequence: 999, trace: 'retained' },
    cache = new BehaviorSubject({ id: 0 })
  const body = { error: { code: 34, message: 'rejected', data: null }, meta }
  assert.throws(
    () => rpcResponseResult(body),
    error => error.code === 34 && error.meta === meta,
  )
  await assert.rejects(
    localRpcRequest(
      {},
      { message: async () => body },
      { lease: async () => lease() },
      identity,
      cache,
      { method: 'example', params: {} },
    ),
    error => error.code === 34 && error.meta === meta,
  )
  assert.equal(cache.observed, false)
})

test('authenticated stream admission captures intent and binds identity and operation lifetimes', async () => {
  const acquired = Promise.withResolvers(),
    user = new AbortController(),
    device = new AbortController(),
    params = { limit: 400 }
  let captured
  const transport = authenticatedRpcTransport(
    {
      stream: async (body, actual, options) => {
        captured = { body, actual, options }
        return {
          initial: 'accepted',
          items: (async function* () {
            return null
          })(),
        }
      },
    },
    { lease: () => acquired.promise },
    identity,
  )
  const pending = transport.stream('server.logs.follow', params, {
    signal: user.signal,
  })
  params.limit = 0
  acquired.resolve({ key: {}, signal: device.signal })
  const response = await pending
  assert.equal(JSON.parse(captured.body).params.limit, 400)
  assert.equal(captured.options.openObserver, identity.openObserver)
  assert.equal(response.signal, captured.options.signal)
  user.abort()
  assert.equal(response.signal.aborted, true)
  assert.equal(device.signal.aborted, false)
})

test('disposed acquisition never dispatches and missing-key streams reject normally without HTTP', async () => {
  const acquired = Promise.withResolvers(),
    abort = new AbortController()
  const sockets = { stream: () => assert.fail('must not dispatch') }
  const pending = authenticatedRpcTransport(
    sockets,
    { lease: () => acquired.promise },
    identity,
  ).stream('server.logs.follow', {}, { signal: abort.signal })
  abort.abort()
  acquired.resolve(lease())
  await assert.rejects(pending, { name: 'AbortError' })
  await assert.rejects(
    authenticatedRpcTransport(
      sockets,
      { lease: async () => null },
      identity,
    ).stream('server.logs.follow', {}),
    error => error.code === 34,
  )
})

test('fulfilled and absent fences release abort listeners; already aborted fences cannot succeed', async () => {
  const abort = new AbortController(),
    cache = new BehaviorSubject({ id: 0 })
  await waitForPatchSequence(cache, {}, abort.signal)
  assert.equal(getEventListeners(abort.signal, 'abort').length, 0)
  const pending = waitForPatchSequence(
    cache,
    { patchSequence: 1 },
    abort.signal,
  )
  cache.next({ id: 1 })
  await pending
  assert.equal(getEventListeners(abort.signal, 'abort').length, 0)
  abort.abort()
  await assert.rejects(waitForPatchSequence(cache, {}, abort.signal), {
    name: 'AbortError',
  })
})
