import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { Subject } from 'rxjs'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import './typescript-loader.mjs'
const { rpcResponseStream } = await import('../src/services/rpc-stream.ts')

function fixture(options) {
  const source = new Subject()
  const pending = rpcResponseStream(source, options)
  return {
    source,
    pending,
    push(value) {
      source.next({
        value,
        bytes: new TextEncoder().encode(JSON.stringify(value)).byteLength,
      })
    },
  }
}
const version = SESSION_PROFILE.transport.rpcStreamVersion
const initial = { jsonrpc: '2.0', result: null }
const event = result => ({ rpcstream: version, result, end: false })
const terminal = result => ({ rpcstream: version, result, end: true })

test('reserved terminal delivery drains a queued backlog and returns its distinct final result', async () => {
  const f = fixture()
  f.push(initial)
  for (let n = 0; n < 64; n++) f.push(event(n))
  f.push(terminal({ outcome: 'finished' }))
  const { items } = await f.pending
  for (let n = 0; n < 64; n++)
    assert.deepEqual(await items.next(), { done: false, value: n })
  assert.deepEqual(await items.next(), {
    done: true,
    value: { outcome: 'finished' },
  })
  assert.equal(f.source.observers.length, 0)
})

test('byte bounds apply independently of item counts, including multibyte text', async () => {
  const f = fixture()
  f.push(initial)
  const { items } = await f.pending
  const text = '😀'.repeat(SESSION_PROFILE.transport.maxIncomingBytes / 8)
  f.push(event(text))
  f.push(event(text))
  await assert.rejects(
    items.next(),
    error => error.code === SESSION_PROFILE.transport.rpcStreamCancelledCode,
  )
  assert.equal(f.source.observers.length, 0)
})

test('abort before admission never subscribes, while abort during initialization releases the source', async () => {
  const abort = new AbortController()
  abort.abort()
  const f = fixture({ signal: abort.signal })
  await assert.rejects(f.pending, { name: 'AbortError' })
  assert.equal(f.source.observers.length, 0)
  const active = new AbortController()
  const pending = fixture({ signal: active.signal })
  const rejected = assert.rejects(pending.pending, { name: 'AbortError' })
  active.abort()
  await rejected
  assert.equal(pending.source.observers.length, 0)
})

test('abort wakes a pending next and discards accepted items even after the wire terminal', async () => {
  const abort = new AbortController()
  const f = fixture({ signal: abort.signal })
  f.push(initial)
  const { items } = await f.pending
  const next = assert.rejects(items.next(), { name: 'AbortError' })
  abort.abort()
  await next
  assert.equal(f.source.observers.length, 0)
  const laterAbort = new AbortController()
  const later = fixture({ signal: laterAbort.signal })
  later.push(initial)
  later.push(event('stale'))
  later.push(terminal('complete'))
  const response = await later.pending
  laterAbort.abort()
  await assert.rejects(response.items.next(), { name: 'AbortError' })
})

test('initial application denial releases the operation and preserves code and metadata', async () => {
  const f = fixture()
  const meta = { gitHash: 'firmware' }
  const rejected = assert.rejects(
    f.pending,
    error => error.code === 34 && error.meta === meta,
  )
  f.push({ jsonrpc: '2.0', error: { code: 34, message: 'denied' }, meta })
  await rejected
  assert.equal(f.source.observers.length, 0)
})

test('iterator return wakes a pending read and throw disposes before the first read', async () => {
  const f = fixture()
  f.push(initial)
  const { items } = await f.pending
  const next = assert.rejects(items.next(), { name: 'AbortError' })
  assert.deepEqual(await items.return('disposed'), {
    value: 'disposed',
    done: true,
  })
  await next
  assert.equal(f.source.observers.length, 0)
  const early = fixture()
  early.push(initial)
  const response = await early.pending
  const error = new Error('caller stopped')
  await assert.rejects(response.items.throw(error), value => value === error)
  assert.equal(early.source.observers.length, 0)
})

test('ordinary source completion is transport loss, never application success', async () => {
  const f = fixture()
  f.push(initial)
  f.push(event('discarded'))
  const { items } = await f.pending
  f.source.complete()
  await assert.rejects(items.next(), error => error.code === 0)
})
