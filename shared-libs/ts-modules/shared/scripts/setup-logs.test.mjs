import assert from 'node:assert/strict'
import test from 'node:test'
import './typescript-loader.mjs'
const { SetupLogsService, provideSetupLogsService } =
  await import('../src/services/setup-logs.service.ts')
const { pauseFor } = await import('../src/util/misc.util.ts')
const flush = () => new Promise(setImmediate)
const entry = message => ({
  timestamp: '2026-11-12T00:00:00Z',
  message,
  bootId: 'boot',
})
const response = items => ({
  initial: { startCursor: 'cursor', guid: null },
  items,
})
function observe(t, follow) {
  t.mock.timers.enable({ apis: ['setInterval', 'setTimeout'] })
  const service = new SetupLogsService(follow)
  const values = []
  const sub = service.subscribe(value => values.push(value))
  t.after(() => sub.unsubscribe())
  return { service, sub, values }
}

test('startup logs disposal aborts acquisition and cannot adopt a late initial reply', async t => {
  const pending = Promise.withResolvers()
  let signal,
    started = 0,
    calls = 0
  const f = observe(t, options => {
    calls++
    signal = options.signal
    return pending.promise
  })
  f.sub.unsubscribe()
  assert.equal(signal.aborted, true)
  pending.resolve(
    response(
      (async function* () {
        started++
        yield entry('late')
        return null
      })(),
    ),
  )
  await flush()
  t.mock.timers.tick(10000)
  await flush()
  assert.equal(started, 0)
  assert.equal(calls, 1)
  assert.deepEqual(f.values, [])
})

test('startup logs preserve 500ms batching and ordered ANSI output, then cancel the iterator', async t => {
  let signal,
    disposed = 0
  const f = observe(t, async options => {
    signal = options.signal
    return response(
      (async function* () {
        try {
          yield entry('first')
          yield entry('second')
          await pauseFor(10000, signal)
          return null
        } finally {
          disposed++
        }
      })(),
    )
  })
  await flush()
  t.mock.timers.tick(499)
  assert.deepEqual(f.values, [])
  t.mock.timers.tick(1)
  assert.equal(f.values.length, 1)
  assert.ok(f.values[0][0].indexOf('first') < f.values[0][0].indexOf('second'))
  f.sub.unsubscribe()
  await flush()
  assert.equal(signal.aborted, true)
  assert.equal(disposed, 1)
  t.mock.timers.tick(10000)
  await flush()
  assert.equal(f.values.length, 1)
})

test('startup logs resubscribe only after the existing normal-terminal delay with a fresh signal', async t => {
  const signals = []
  const f = observe(t, async ({ signal }) => {
    signals.push(signal)
    return response(
      (async function* () {
        yield entry('finished')
        return null
      })(),
    )
  })
  await flush()
  assert.equal(f.values.length, 1)
  assert.equal(signals[0].aborted, true)
  t.mock.timers.tick(499)
  await flush()
  assert.equal(signals.length, 1)
  t.mock.timers.tick(1)
  await flush()
  assert.equal(signals.length, 2)
  assert.notEqual(signals[0], signals[1])
})

test('startup logs retry an application terminal error but disposal cancels the recovery wait', async t => {
  let calls = 0
  const f = observe(t, async () => {
    calls++
    return response(
      (async function* () {
        throw new Error('late journal failure')
      })(),
    )
  })
  await flush()
  t.mock.timers.tick(500)
  await flush()
  assert.equal(calls, 2)
  f.sub.unsubscribe()
  t.mock.timers.tick(10000)
  await flush()
  assert.equal(calls, 2)
})

test('startup logs acquisition failures retain the existing read-only retry policy', async t => {
  let calls = 0
  observe(t, async () => {
    calls++
    throw { code: 0, message: 'lost' }
  })
  await flush()
  t.mock.timers.tick(499)
  await flush()
  assert.equal(calls, 1)
  t.mock.timers.tick(1)
  await flush()
  assert.equal(calls, 2)
})

test('startup logs consumers own separate cancellation lifetimes', async t => {
  const signals = []
  const pending = Promise.withResolvers()
  const f = observe(t, ({ signal }) => {
    signals.push(signal)
    return pending.promise
  })
  const second = f.service.subscribe()
  t.after(() => second.unsubscribe())
  f.sub.unsubscribe()
  assert.equal(signals[0].aborted, true)
  assert.equal(signals[1].aborted, false)
  second.unsubscribe()
  assert.equal(signals[1].aborted, true)
})

test('startup logs provider passes its cancellation options to the supplied API operation', async t => {
  class Api {}
  let received
  const provider = provideSetupLogsService(Api, (api, options) => {
    received = { api, options }
    return new Promise(() => {})
  })
  const api = new Api()
  const service = provider.useFactory(api)
  const sub = service.subscribe()
  t.after(() => sub.unsubscribe())
  assert.equal(received.api, api)
  assert.equal(received.options.signal.aborted, false)
  sub.unsubscribe()
  assert.equal(received.options.signal.aborted, true)
})
