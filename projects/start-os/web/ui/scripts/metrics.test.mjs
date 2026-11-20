import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { Injector } from '@angular/core'
import { BehaviorSubject, Subject } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'
const shared = new URL(
  '../../../../../shared-libs/ts-modules/shared/src/',
  import.meta.url,
)
loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      `export { withAbortSignal } from '${new URL('util/with-abort.ts', shared)}'`,
    ],
    ['src/app/services/api/embassy-api.service', 'export class ApiService {}'],
    ['src/app/services/auth.service', 'export class AuthService {}'],
    [
      'src/app/services/connection.service',
      'export class ConnectionService {}',
    ],
  ]),
)
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { AuthService } = await import('src/app/services/auth.service')
const { ConnectionService } =
  await import('src/app/services/connection.service')
const { MetricsService } =
  await import('../src/app/routes/portal/routes/metrics/metrics.service.ts')
const { Metrics } =
  await import('../../../../../shared-libs/ts-modules/start-core/lib/osBindings/types.schemas.ts')
const { rpcResponseStream } = await import(
  new URL('services/rpc-stream.ts', shared)
)
const { SESSION_PROFILE } =
  await import('@start9labs/start-core/osBindings/session-profile')
const flush = () => new Promise(setImmediate)
const percent = { value: '0', unit: '%' },
  mib = { value: '0', unit: 'MiB' },
  gib = { value: '0', unit: 'GB' }
const sample = Metrics.parse({
  general: { temperature: null },
  memory: {
    percentageUsed: percent,
    total: mib,
    available: mib,
    used: mib,
    zramTotal: mib,
    zramAvailable: mib,
    zramUsed: mib,
  },
  cpu: {
    percentageUsed: percent,
    idle: percent,
    userSpace: percent,
    kernelSpace: percent,
    wait: percent,
  },
  disk: { percentageUsed: percent, used: gib, available: gib, capacity: gib },
})
function fixture(t) {
  const authenticated = new BehaviorSubject(true),
    connection = new BehaviorSubject(false),
    requests = []
  const api = {
    streams: {
      server: {
        metrics: {
          follow: (_, options) => {
            const frames = new Subject()
            requests.push({ frames, signal: options.signal })
            return rpcResponseStream(frames, options)
          },
        },
      },
    },
  }
  const injector = Injector.create({
    providers: [
      MetricsService,
      { provide: ApiService, useValue: api },
      { provide: AuthService, useValue: { isVerified$: authenticated } },
      { provide: ConnectionService, useValue: connection },
    ],
  })
  const service = injector.get(MetricsService),
    subscriptions = []
  t.after(() => {
    subscriptions.forEach(sub => sub.unsubscribe())
    injector.destroy()
  })
  const observe = () => {
    const values = [],
      sub = service.subscribe(value => values.push(value))
    subscriptions.push(sub)
    return { values, sub }
  }
  const send = (index, value) =>
    requests[index].frames.next({ value, bytes: JSON.stringify(value).length })
  const resolve = async index => {
    send(index, { jsonrpc: '2.0', result: { guid: null, metrics: sample } })
    await flush()
  }
  const item = (index, metrics) =>
    send(index, {
      rpcstream: SESSION_PROFILE.transport.rpcStreamVersion,
      end: false,
      result: metrics,
    })
  const finish = index =>
    send(index, {
      rpcstream: SESSION_PROFILE.transport.rpcStreamVersion,
      end: true,
      result: null,
    })
  return {
    authenticated,
    connection,
    requests,
    observe,
    resolve,
    item,
    finish,
    send,
  }
}

test('metrics shares one typed source and cancels it when the last consumer leaves', async t => {
  const f = fixture(t),
    a = f.observe(),
    b = f.observe()
  assert.equal(f.requests.length, 1)
  await f.resolve(0)
  assert.equal(f.requests[0].frames.observers.length, 1)
  assert.deepEqual(a.values, [sample])
  assert.deepEqual(b.values, [sample])
  a.sub.unsubscribe()
  assert.equal(f.requests[0].frames.observers.length, 1)
  b.sub.unsubscribe()
  assert.equal(f.requests[0].frames.observers.length, 0)
  assert.equal(f.requests[0].signal.aborted, true)
  assert.equal(f.authenticated.observers.length, 0)
})

test('metrics disposal before initialization cancels its source and cannot adopt late data', async t => {
  const f = fixture(t),
    a = f.observe()
  a.sub.unsubscribe()
  await f.resolve(0)
  assert.equal(f.requests[0].frames.observers.length, 0)
  assert.equal(f.requests[0].signal.aborted, true)
  assert.deepEqual(a.values, [])
})

test('metrics disposal cancels recovery and connectivity cannot reopen it', async t => {
  const f = fixture(t),
    a = f.observe()
  await f.resolve(0)
  f.requests[0].frames.error({ code: 0, message: 'lost' })
  await flush()
  assert.equal(f.connection.observers.length, 1)
  a.sub.unsubscribe()
  assert.equal(f.connection.observers.length, 0)
  f.connection.next(true)
  await flush()
  assert.equal(f.requests.length, 1)
})

test('metrics observes only verified authentication and login acquires a fresh source', async t => {
  const f = fixture(t)
  f.authenticated.next(false)
  const a = f.observe()
  assert.equal(f.requests.length, 0)
  f.authenticated.next(true)
  await f.resolve(0)
  f.authenticated.next(false)
  assert.equal(f.requests[0].frames.observers.length, 0)
  assert.equal(f.requests[0].signal.aborted, true)
  f.connection.next(true)
  await flush()
  assert.equal(f.requests.length, 1)
  f.authenticated.next(true)
  await f.resolve(1)
  assert.deepEqual(a.values, [sample, sample])
  assert.notEqual(f.requests[0].signal, f.requests[1].signal)
})

test('metrics logout cancels pending initialization and recovery', async t => {
  const f = fixture(t),
    a = f.observe()
  f.authenticated.next(false)
  await f.resolve(0)
  assert.equal(f.requests[0].frames.observers.length, 0)
  f.authenticated.next(true)
  await f.resolve(1)
  f.requests[1].frames.error({ code: 0, message: 'lost' })
  await flush()
  assert.equal(f.connection.observers.length, 1)
  f.authenticated.next(false)
  assert.equal(f.connection.observers.length, 0)
  f.connection.next(true)
  await flush()
  assert.equal(f.requests.length, 2)
  assert.deepEqual(a.values, [sample])
})

test('metrics reopens on existing connectivity recovery without replaying a lost source', async t => {
  const f = fixture(t),
    a = f.observe()
  await f.resolve(0)
  f.requests[0].frames.error({ code: 0, message: 'lost' })
  await flush()
  assert.equal(f.requests.length, 1)
  f.connection.next(true)
  await f.resolve(1)
  assert.deepEqual(a.values, [sample, sample])
  assert.notEqual(f.requests[0].signal, f.requests[1].signal)
})

test('metrics starts fresh after disposal without replaying a previous-session snapshot', async t => {
  const f = fixture(t),
    a = f.observe()
  await f.resolve(0)
  a.sub.unsubscribe()
  const b = f.observe()
  assert.deepEqual(b.values, [])
  assert.equal(f.requests.length, 2)
  await f.resolve(1)
  assert.deepEqual(b.values, [sample])
})

test('metrics publishes typed items after the initial snapshot and drains before normal-end recovery', async t => {
  const f = fixture(t),
    a = f.observe()
  await f.resolve(0)
  const update = structuredClone(sample)
  update.cpu.percentageUsed.value = '42'
  f.item(0, update)
  f.finish(0)
  await flush()
  assert.deepEqual(a.values, [sample, update])
  assert.equal(f.requests.length, 2)
  assert.equal(f.requests[0].signal.aborted, true)
  await f.resolve(1)
  assert.deepEqual(a.values, [sample, update, sample])
})

test('metrics initial application errors use existing recovery and disposal cancels the wait', async t => {
  const f = fixture(t),
    a = f.observe()
  f.send(0, {
    jsonrpc: '2.0',
    error: { code: 6, message: 'No Metrics Found', data: null },
  })
  await flush()
  assert.equal(f.connection.observers.length, 1)
  assert.equal(f.requests[0].signal.aborted, true)
  a.sub.unsubscribe()
  f.connection.next(true)
  await flush()
  assert.equal(f.requests.length, 1)
  assert.deepEqual(a.values, [])
})
