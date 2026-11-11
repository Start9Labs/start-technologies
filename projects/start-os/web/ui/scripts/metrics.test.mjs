import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { Injector } from '@angular/core'
import { BehaviorSubject, Subject } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
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
const flush = () => new Promise(setImmediate)
const percent = { value: '0', unit: '%' }
const mib = { value: '0', unit: 'MiB' }
const gib = { value: '0', unit: 'GB' }
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
  const authenticated = new BehaviorSubject(true)
  const connection = new BehaviorSubject(false)
  const requests = [],
    sockets = []
  const api = {
    rpc: {
      server: {
        metrics: {
          follow: () => {
            const pending = Promise.withResolvers()
            requests.push(pending)
            return pending.promise
          },
        },
      },
    },
    openWebsocket$: guid => {
      const frames = new Subject()
      sockets.push({ guid, frames })
      return frames
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
  const service = injector.get(MetricsService)
  const subscriptions = []
  t.after(() => {
    for (const sub of subscriptions) sub.unsubscribe()
    injector.destroy()
  })
  const observe = () => {
    const values = []
    const sub = service.subscribe(value => values.push(value))
    subscriptions.push(sub)
    return { values, sub }
  }
  const resolve = async index => {
    const guid = 'A'.repeat(31) + 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'[index]
    requests[index].resolve({ guid, metrics: sample })
    await flush()
    return guid
  }
  return { authenticated, connection, requests, sockets, observe, resolve }
}

test('metrics shares one live source and releases it when the last consumer leaves', async t => {
  const f = fixture(t)
  const a = f.observe(),
    b = f.observe()
  assert.equal(f.requests.length, 1)
  await f.resolve(0)
  assert.equal(f.sockets.length, 1)
  assert.equal(f.sockets[0].frames.observers.length, 1)
  assert.deepEqual(a.values, [sample])
  assert.deepEqual(b.values, [sample])
  a.sub.unsubscribe()
  assert.equal(f.sockets[0].frames.observers.length, 1)
  b.sub.unsubscribe()
  assert.equal(f.sockets[0].frames.observers.length, 0)
  assert.equal(f.authenticated.observers.length, 0)
})

test('metrics disposal before the initial reply cannot adopt its late GUID', async t => {
  const f = fixture(t)
  const a = f.observe()
  a.sub.unsubscribe()
  await f.resolve(0)
  assert.equal(f.sockets.length, 0)
  assert.deepEqual(a.values, [])
})

test('metrics disposal cancels recovery and connectivity cannot reopen it', async t => {
  const f = fixture(t)
  const a = f.observe()
  await f.resolve(0)
  f.sockets[0].frames.error({ code: 0, message: 'lost' })
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
  assert.equal(f.sockets[0].frames.observers.length, 0)
  f.connection.next(true)
  await flush()
  assert.equal(f.requests.length, 1)
  f.authenticated.next(true)
  await f.resolve(1)
  assert.deepEqual(a.values, [sample, sample])
  assert.notEqual(f.sockets[0].guid, f.sockets[1].guid)
})

test('metrics logout abandons pending initialization and recovery', async t => {
  const f = fixture(t)
  const a = f.observe()
  f.authenticated.next(false)
  await f.resolve(0)
  assert.equal(f.sockets.length, 0)
  f.authenticated.next(true)
  await f.resolve(1)
  f.sockets[0].frames.error({ code: 0, message: 'lost' })
  assert.equal(f.connection.observers.length, 1)
  f.authenticated.next(false)
  assert.equal(f.connection.observers.length, 0)
  f.connection.next(true)
  await flush()
  assert.equal(f.requests.length, 2)
  assert.deepEqual(a.values, [sample])
})

test('metrics reopens on existing connectivity recovery, not on a lost source replay', async t => {
  const f = fixture(t)
  const a = f.observe()
  await f.resolve(0)
  f.sockets[0].frames.error({ code: 0, message: 'lost' })
  await flush()
  assert.equal(f.requests.length, 1)
  f.connection.next(true)
  await f.resolve(1)
  assert.deepEqual(a.values, [sample, sample])
  assert.notEqual(f.sockets[0].guid, f.sockets[1].guid)
})

test('metrics starts fresh after observer disposal without replaying the previous session snapshot', async t => {
  const f = fixture(t)
  const a = f.observe()
  await f.resolve(0)
  a.sub.unsubscribe()
  const b = f.observe()
  assert.deepEqual(b.values, [])
  assert.equal(f.requests.length, 2)
  await f.resolve(1)
  assert.deepEqual(b.values, [sample])
})
