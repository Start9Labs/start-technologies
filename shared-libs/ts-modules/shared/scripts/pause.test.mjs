import assert from 'node:assert/strict'
import test from 'node:test'
import { getEventListeners } from 'node:events'
import './typescript-loader.mjs'

const { pauseFor } = await import('../src/util/misc.util.ts')

test('pause preserves its deadline and releases its abort observer after fulfillment', async context => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const abort = new AbortController()
  let complete = false
  const waiting = pauseFor(50, abort.signal).then(() => (complete = true))
  context.mock.timers.tick(49)
  await Promise.resolve()
  assert.equal(complete, false)
  context.mock.timers.tick(1)
  await waiting
  assert.equal(complete, true)
  assert.equal(getEventListeners(abort.signal, 'abort').length, 0)
})

test(
  'pause rejects a pending wait immediately with its cancellation reason',
  { timeout: 1000 },
  async () => {
    const abort = new AbortController()
    const reason = new Error('canceled pause')
    const rejected = assert.rejects(
      pauseFor(60_000, abort.signal),
      error => error === reason,
    )
    abort.abort(reason)
    await rejected
    assert.equal(getEventListeners(abort.signal, 'abort').length, 0)
  },
)

test('pause rejects an already canceled wait and preserves the default timer contract', async context => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const abort = new AbortController()
  abort.abort()
  await assert.rejects(pauseFor(60_000, abort.signal), { name: 'AbortError' })
  const waiting = pauseFor(10)
  context.mock.timers.tick(10)
  await waiting
})
