import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { BehaviorSubject } from 'rxjs'
import './typescript-loader.mjs'
const { withLegacyPatchSequence, waitForPatchSequence } =
  await import('../src/services/rpc-response.ts')

test('patch sequence fences preserve zero and skip absent metadata', async () => {
  const cache = new BehaviorSubject({ id: 0 })
  for (const meta of [
    undefined,
    {},
    { patchSequence: null },
    { patchSequence: 0 },
  ]) {
    await waitForPatchSequence(cache, meta)
    assert.equal(cache.observed, false)
  }
})

test('patch sequence fences wait for the committed revision and release their subscription', async () => {
  const cache = new BehaviorSubject({ id: 1 })
  let completed = false
  const pending = waitForPatchSequence(cache, { patchSequence: 3 }).then(() => {
    completed = true
  })
  assert.equal(cache.observed, true)
  cache.next({ id: 2 })
  await Promise.resolve()
  assert.equal(completed, false)
  cache.next({ id: 3 })
  await pending
  assert.equal(cache.observed, false)
})

test('JSON metadata is authoritative, including sequence zero', () => {
  for (const patchSequence of [0, 17]) {
    const response = {
      jsonrpc: '2.0',
      id: 1,
      result: null,
      meta: { patchSequence, trace: 'keep' },
    }
    assert.equal(withLegacyPatchSequence(response, '42'), response)
  }
})

test('old HTTP sequences join existing metadata without mutating the response', () => {
  const response = {
    jsonrpc: '2.0',
    id: null,
    result: { saved: true },
    meta: { trace: 'keep' },
  }
  const normalized = withLegacyPatchSequence(response, '42')
  assert.deepEqual(normalized, {
    ...response,
    meta: { trace: 'keep', patchSequence: 42 },
  })
  assert.deepEqual(response.meta, { trace: 'keep' })
  assert.equal(normalized.result, response.result)
})

test('missing HTTP headers do not invent metadata', () => {
  const response = { jsonrpc: '2.0', id: 2, result: 'done' }
  assert.equal(withLegacyPatchSequence(response, null), response)
})

test('nullable sequences have the same compatibility behavior as omitted fields', () => {
  const response = {
    jsonrpc: '2.0',
    id: 2,
    result: 'done',
    meta: { patchSequence: null },
  }
  assert.equal(withLegacyPatchSequence(response, null), response)
  assert.equal(withLegacyPatchSequence(response, '17').meta.patchSequence, 17)
})

test('compatibility preserves errors and can carry sequence zero', () => {
  const error = { code: 1, message: 'failed' }
  const response = { jsonrpc: '2.0', id: 3, error }
  assert.deepEqual(withLegacyPatchSequence(response, '0'), {
    ...response,
    meta: { patchSequence: 0 },
  })
  assert.equal(withLegacyPatchSequence(response, '8').error, error)
})
