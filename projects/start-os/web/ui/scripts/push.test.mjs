import assert from 'node:assert/strict'
import { createECDH } from 'node:crypto'
import test from 'node:test'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { SwPush } from '@angular/service-worker'
import { Observable, Subject } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      'export class ErrorService {}\nexport class i18nPipe {}',
    ],
    ['@ng-web-apis/common', 'export const WA_WINDOW = Symbol("window")'],
  ]),
)
const { PushService, BrowserPushSync } =
  await import('src/app/services/push.service')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { ErrorService, i18nPipe } = await import('@start9labs/shared')
const { WA_WINDOW } = await import('@ng-web-apis/common')

const ecdh = createECDH('prime256v1')
ecdh.setPrivateKey(Buffer.alloc(32, 1))
const publicKey = ecdh.getPublicKey().toString('base64url')
const keys = {
  p256dh: publicKey,
  auth: Buffer.alloc(16, 2).toString('base64url'),
}
function subscription(
  endpoint = 'http://192.168.1.4:8080/push/browser',
  key = publicKey,
) {
  return {
    endpoint,
    options: {
      applicationServerKey: Uint8Array.from(Buffer.from(key, 'base64url'))
        .buffer,
    },
    toJSON: () => ({ endpoint, keys }),
  }
}
function deferred() {
  let resolve, reject
  const promise = new Promise((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}
function fixture(
  t,
  { current = null, supported = true, permission = 'granted' } = {},
) {
  const calls = []
  const errors = []
  const subscriptions = new Map()
  const sw = {
    isEnabled: supported,
    pushSubscriptionChanges: new Subject(),
    subscription: new Observable(observer => {
      observer.next(current)
      observer.complete()
    }),
    requestSubscription: async options => {
      calls.push(['browser.subscribe', options])
      return (current ??= subscription())
    },
    unsubscribe: async () => {
      calls.push(['browser.unsubscribe'])
      current = null
    },
  }
  const rpc = {
    key: async () => {
      calls.push(['key'])
      return publicKey
    },
    subscribe: async params => {
      calls.push(['subscribe', params])
      subscriptions.set(params.endpoint, params)
    },
    unsubscribe: async ({ endpoint }) => {
      calls.push(['unsubscribe', endpoint])
      subscriptions.delete(endpoint)
    },
  }
  const injector = createEnvironmentInjector([
    PushService,
    { provide: ApiService, useValue: { rpc: { notification: { push: rpc } } } },
    { provide: SwPush, useValue: sw },
    {
      provide: ErrorService,
      useValue: { handleError: error => errors.push(error) },
    },
    { provide: i18nPipe, useValue: { transform: text => text } },
    {
      provide: WA_WINDOW,
      useValue: {
        PushManager: supported ? class {} : undefined,
        Notification: {
          requestPermission: () => {
            calls.push(['permission'])
            return Promise.resolve(permission)
          },
        },
        btoa,
      },
    },
  ])
  t.after(() => {
    if (!injector.destroyed) injector.destroy()
  })
  const service = runInInjectionContext(injector, () =>
    injector.get(PushService),
  )
  return {
    service,
    sw,
    rpc,
    injector,
    calls,
    errors,
    subscriptions,
    setCurrent: value => (current = value),
    current: () => current,
  }
}

test('unsupported browsers never request permission or contact the server', async t => {
  const f = fixture(t, { supported: false })
  await f.service.start()
  assert.equal(f.service.available, false)
  assert.deepEqual(f.calls, [])
})

test('opening StartOS does not enroll an unsubscribed browser or prompt permission', async t => {
  const f = fixture(t)
  await f.service.start()
  assert.equal(f.service.ready(), true)
  assert.equal(f.service.enabled(), false)
  assert.deepEqual(f.calls, [])
})

test('enable prompts within the click turn and forwards browser keys and LAN endpoint unchanged', async t => {
  const f = fixture(t)
  const enabling = f.service.enable()
  assert.deepEqual(f.calls, [['permission']])
  await enabling
  assert.deepEqual(
    f.calls.map(([name]) => name),
    ['permission', 'key', 'browser.subscribe', 'subscribe'],
  )
  assert.deepEqual(
    [...f.subscriptions.values()],
    [{ endpoint: f.current().endpoint, keys }],
  )
  assert.equal(f.service.enabled(), true)
})

test('permission denial never creates a subscription or contacts the server', async t => {
  const f = fixture(t, { permission: 'denied' })
  await assert.rejects(f.service.enable(), /Notifications are blocked/)
  assert.deepEqual(f.calls, [['permission']])
  assert.equal(f.service.enabled(), false)
})

test('registration failures remain retryable using the browser-owned subscription', async t => {
  const f = fixture(t)
  const save = f.rpc.subscribe
  f.rpc.subscribe = async () => {
    throw new Error('offline')
  }
  await assert.rejects(f.service.enable(), /offline/)
  assert.ok(f.current())
  assert.equal(f.service.enabled(), false)
  f.rpc.subscribe = save
  await f.service.enable()
  assert.equal(f.service.enabled(), true)
  assert.equal(f.subscriptions.size, 1)
})

test('existing browser consent restores delivery after a server restart or closed-page renewal', async t => {
  const current = subscription('https://self-hosted.example/push/renewed')
  const f = fixture(t, { current })
  await f.service.start()
  assert.equal(f.service.enabled(), true)
  assert.equal(f.subscriptions.has(current.endpoint), true)
  assert.deepEqual(
    f.calls.map(([name]) => name),
    ['key', 'subscribe'],
  )
})

test('a changed server key replaces the old native subscription before enrollment', async t => {
  const old = subscription(
    'https://push.example/old',
    Buffer.alloc(65, 3).toString('base64url'),
  )
  const f = fixture(t, { current: old })
  await f.service.start()
  assert.equal(f.service.enabled(), true)
  assert.deepEqual(
    f.calls.map(([name]) => name),
    [
      'key',
      'browser.unsubscribe',
      'unsubscribe',
      'browser.subscribe',
      'subscribe',
    ],
  )
  assert.notEqual(f.current().endpoint, old.endpoint)
})

test('disable revokes browser consent before a failing server request', async t => {
  const f = fixture(t, { current: subscription() })
  await f.service.start()
  f.calls.length = 0
  f.rpc.unsubscribe = async () => {
    f.calls.push(['unsubscribe'])
    throw new Error('offline')
  }
  await assert.rejects(f.service.disable(), /offline/)
  assert.deepEqual(
    f.calls.map(([name]) => name),
    ['browser.unsubscribe', 'unsubscribe'],
  )
  assert.equal(f.current(), null)
  assert.equal(f.service.enabled(), false)
  await f.service.start()
  assert.equal(f.service.enabled(), false)
})

test('failed native unsubscribe leaves consent and server enrollment intact', async t => {
  const f = fixture(t, { current: subscription() })
  await f.service.start()
  f.calls.length = 0
  f.sw.unsubscribe = async () => {
    throw new Error('browser unsubscribe failed')
  }
  await assert.rejects(f.service.disable(), /browser unsubscribe failed/)
  assert.equal(f.service.enabled(), true)
  assert.equal(f.subscriptions.size, 1)
  assert.deepEqual(f.calls, [])
})

test('browser expiration clears enrollment without requesting permission', async t => {
  const old = subscription()
  const f = fixture(t, { current: old })
  await f.service.start()
  f.calls.length = 0
  f.setCurrent(null)
  f.sw.pushSubscriptionChanges.next({
    oldSubscription: old,
    newSubscription: null,
  })
  await new Promise(setImmediate)
  assert.equal(f.service.enabled(), false)
  assert.equal(f.subscriptions.size, 0)
  assert.deepEqual(f.calls, [['unsubscribe', old.endpoint]])
})

test('initial synchronization failure reports the error and leaves enable available for retry', async t => {
  const f = fixture(t, { current: subscription() })
  const save = f.rpc.subscribe
  f.rpc.subscribe = async () => {
    throw new Error('offline')
  }
  await f.service.start()
  assert.equal(f.service.ready(), true)
  assert.equal(f.service.enabled(), false)
  assert.equal(f.errors.length, 1)
  f.rpc.subscribe = save
  await f.service.enable()
  assert.equal(f.service.enabled(), true)
})

test('renewal followed by disable cannot leave a registered replacement', async t => {
  const old = subscription('https://push.example/old')
  const fresh = subscription('https://push.example/new')
  const f = fixture(t, { current: old })
  await f.service.start()
  const saved = deferred()
  const save = f.rpc.subscribe
  f.rpc.subscribe = async params => {
    await saved.promise
    return save(params)
  }
  f.setCurrent(fresh)
  f.sw.pushSubscriptionChanges.next({
    oldSubscription: old,
    newSubscription: fresh,
  })
  const disabled = f.service.disable()
  saved.resolve()
  await disabled
  assert.equal(f.current(), null)
  assert.equal(f.service.enabled(), false)
  assert.equal(f.subscriptions.size, 0)
})

test('a delayed renewal event cannot undo a completed disable', async t => {
  const old = subscription()
  const f = fixture(t, { current: old })
  await f.service.start()
  await f.service.disable()
  f.calls.length = 0
  f.sw.pushSubscriptionChanges.next({
    oldSubscription: old,
    newSubscription: subscription('https://push.example/stale'),
  })
  await new Promise(setImmediate)
  assert.deepEqual(f.calls, [['unsubscribe', old.endpoint]])
  assert.equal(f.service.enabled(), false)
  assert.equal(f.subscriptions.size, 0)
})

test('renewal errors are reported and do not terminate subsequent renewals', async t => {
  const f = fixture(t, { current: subscription() })
  await f.service.start()
  const save = f.rpc.subscribe
  f.rpc.subscribe = async () => {
    throw new Error('offline')
  }
  f.sw.pushSubscriptionChanges.next({
    oldSubscription: f.current(),
    newSubscription: f.current(),
  })
  await new Promise(setImmediate)
  assert.equal(f.errors.length, 1)
  assert.equal(f.service.enabled(), false)
  f.rpc.subscribe = save
  const fresh = subscription('http://[fd00::5]:8080/push/new')
  f.setCurrent(fresh)
  f.sw.pushSubscriptionChanges.next({
    oldSubscription: subscription(),
    newSubscription: fresh,
  })
  await new Promise(setImmediate)
  assert.equal(f.subscriptions.has(fresh.endpoint), true)
})

test('authenticated portal startup synchronizes subscriptions, and destruction stops renewal handling', async t => {
  const f = fixture(t, { current: subscription() })
  runInInjectionContext(f.injector, () => new BrowserPushSync())
  await new Promise(setImmediate)
  assert.equal(f.service.enabled(), true)
  f.injector.destroy()
  f.calls.length = 0
  f.sw.pushSubscriptionChanges.next({
    oldSubscription: subscription(),
    newSubscription: null,
  })
  await new Promise(setImmediate)
  assert.deepEqual(f.calls, [])
})
