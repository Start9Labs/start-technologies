import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { Injector } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { TuiNotificationService } from '@taiga-ui/core'
import { of } from 'rxjs'
import { auth } from '@start9labs/start-core'
import './typescript-loader.mjs'

const { ServerIdentityService } =
  await import('../src/services/server-identity.service.ts')
const { AUTH_KEY_STORAGE_KEY, AUTH_SIGNING_HOST, AuthKeyService } =
  await import('../src/services/auth-key.service.ts')
const { RELATIVE_URL } = await import('../src/tokens/relative-url.ts')

function database() {
  const values = new Map()
  let previous = Promise.resolve()
  return {
    values,
    open() {
      const opening = {}
      queueMicrotask(() => {
        opening.result = {
          close() {},
          transaction() {
            const start = previous
            let finish
            previous = new Promise(resolve => {
              finish = resolve
            })
            const transaction = {}
            const request = operation => {
              const result = {}
              void start.then(() => {
                result.result = operation()
                result.onsuccess?.()
                queueMicrotask(() => {
                  transaction.oncomplete?.()
                  finish()
                })
              })
              return result
            }
            transaction.objectStore = () => ({
              get: key => request(() => values.get(key)),
              put: (value, key) => request(() => values.set(key, value)),
              delete: key => request(() => values.delete(key)),
            })
            return transaction
          },
        }
        opening.onsuccess()
      })
      return opening
    },
  }
}

function fixture(db, { namespace = 'device', url = '/rpc/v1' } = {}) {
  const notifications = []
  const injector = Injector.create({
    providers: [
      ServerIdentityService,
      AuthKeyService,
      {
        provide: WA_WINDOW,
        useValue: {
          indexedDB: db,
          crypto,
          location: { origin: 'https://server.test' },
        },
      },
      { provide: AUTH_KEY_STORAGE_KEY, useValue: namespace },
      { provide: AUTH_SIGNING_HOST, useValue: 'server.test' },
      { provide: RELATIVE_URL, useValue: url },
      {
        provide: TuiNotificationService,
        useValue: {
          open: (_, options) => {
            notifications.push(options)
            return of(undefined)
          },
        },
      },
    ],
  })
  return {
    injector,
    service: injector.get(ServerIdentityService),
    notifications,
  }
}

const first = auth.ed25519PublicKeyDer64(
  Uint8Array.from(
    Buffer.from(
      'd54207da194977dcf46adbfec2bc2e75b52d5a8a42184fedfdc00024f0e3e8da',
      'hex',
    ),
  ),
)
const second = auth.ed25519PublicKeyDer64(new Uint8Array(32).fill(19))
const golden =
  'SHA256:d7e1e084be213b01e506852af8198b9980b43e03749c67d82dcd0903f4001c77'

test('own-server first contact is silent; rotation is remembered across service reload', async () => {
  const db = database()
  const a = fixture(db)
  await a.service.observe(first)
  assert.equal(a.notifications.length, 0)
  assert.deepEqual([...db.values.values()], [golden])
  await a.service.observe(first)
  a.injector.destroy()
  const b = fixture(db)
  await b.service.observe(second)
  assert.equal(b.notifications.length, 1)
  const { data } = b.notifications[0]
  assert.equal(data.target, 'https://server.test/rpc/v1')
  assert.equal(data.previous, golden)
  assert.notEqual(data.current, golden)
  assert.deepEqual([...db.values.values()], [data.current])
  await b.service.observe(second)
  assert.equal(b.notifications.length, 1)
  b.injector.destroy()
})

test('concurrent RPC and continuation observations report one rotation', async () => {
  const db = database()
  const f = fixture(db)
  await f.service.observe(first)
  await Promise.all([
    f.service.observe(second),
    f.service.observe(second),
    f.service.observe(second),
  ])
  assert.equal(f.notifications.length, 1)
  assert.equal(f.notifications[0].data.previous, golden)
  f.injector.destroy()
})

test('server history survives device-key logout and isolates application API namespaces', async () => {
  const db = database()
  const a = fixture(db)
  db.values.set('device', { privateKey: 'opaque device key fixture' })
  await a.service.observe(first)
  await a.injector.get(AuthKeyService).clear()
  assert.equal(db.values.has('device'), false)
  assert.deepEqual([...db.values.values()], [golden])
  for (const options of [{ namespace: 'other' }, { url: '/other/rpc/v1' }]) {
    const b = fixture(db, options)
    await b.service.observe(second)
    assert.equal(b.notifications.length, 0)
    b.injector.destroy()
  }
  assert.equal(db.values.size, 3)
  a.injector.destroy()
})

test('foreign API targets are never remembered or presented as own-server changes', async () => {
  const db = database()
  const f = fixture(db, { url: 'https://registry.test/rpc/v0' })
  await f.service.observe(first)
  await f.service.observe(second)
  assert.equal(db.values.size, 0)
  assert.equal(f.notifications.length, 0)
  f.injector.destroy()
})

test('unavailable browser storage does not reject admission observers or show a false remembered notice', async context => {
  context.mock.method(console, 'warn', () => {})
  const f = fixture({
    open() {
      throw new Error('storage denied')
    },
  })
  await assert.doesNotReject(f.service.observe(first))
  assert.equal(f.notifications.length, 0)
  f.injector.destroy()
})
