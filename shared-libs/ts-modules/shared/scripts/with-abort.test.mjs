import assert from 'node:assert/strict'
import test from 'node:test'
import { EMPTY, Subject } from 'rxjs'
import './typescript-loader.mjs'
const { withAbortSignal } = await import('../src/util/with-abort.ts')

test('observation completion aborts its operation signal', () => {
  let signal
  withAbortSignal(value => {
    signal = value
    return EMPTY
  }).subscribe()
  assert.equal(signal.aborted, true)
})

test('a synchronous operation failure aborts and preserves the error', () => {
  let signal, received
  const failure = new Error('setup failure')
  withAbortSignal(value => {
    signal = value
    throw failure
  }).subscribe({
    error: error => {
      received = error
    },
  })
  assert.equal(received, failure)
  assert.equal(signal.aborted, true)
})

test('subscriptions receive independent signals and disposal releases the source', () => {
  const signals = [],
    source = new Subject()
  const observation = withAbortSignal(signal => {
    signals.push(signal)
    return source
  })
  const first = observation.subscribe(),
    second = observation.subscribe()
  assert.equal(source.observers.length, 2)
  first.unsubscribe()
  assert.equal(signals[0].aborted, true)
  assert.equal(signals[1].aborted, false)
  assert.equal(source.observers.length, 1)
  second.unsubscribe()
  assert.equal(source.observers.length, 0)
  assert.equal(signals[1].aborted, true)
})
