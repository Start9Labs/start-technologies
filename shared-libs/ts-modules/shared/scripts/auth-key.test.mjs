import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { Injector, runInInjectionContext } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { auth } from '@start9labs/start-core'
import './typescript-loader.mjs'

const { AuthKeyService, AUTH_SIGNING_HOST, AUTH_KEY_STORAGE_KEY } =
  await import('../src/services/auth-key.service.ts')

function keys(cached = null) {
  const injector = Injector.create({
    providers: [
      { provide: WA_WINDOW, useValue: {} },
      { provide: AUTH_SIGNING_HOST, useValue: 'server.test' },
      { provide: AUTH_KEY_STORAGE_KEY, useValue: 'test' },
    ],
  })
  const service = runInInjectionContext(injector, () => new AuthKeyService())
  service.cached = cached
  service.idb = async () => undefined
  return service
}

test('logout invalidates an active key lease before persistence finishes', async () => {
  const service = keys(await auth.generateAuthKey())
  const lease = await service.lease()
  let complete
  service.idb = () =>
    new Promise(resolve => {
      complete = resolve
    })
  const clearing = service.clear()
  assert.equal(lease.key.privateKey.extractable, false)
  assert.equal(lease.signal.aborted, true)
  assert.equal(await service.lease(), null)
  complete()
  await clearing
})

test('replacement and failed-login rollback cancel only their own key lifetimes', async () => {
  const original = await auth.generateAuthKey()
  const service = keys(original)
  const before = await service.lease()
  const replacement = await service.create()
  const during = await service.lease()
  assert.equal(before.signal.aborted, true)
  assert.equal(during.key, replacement)
  assert.equal(during.signal.aborted, false)
  await service.rollback()
  const restored = await service.lease()
  assert.equal(during.signal.aborted, true)
  assert.equal(restored.key, original)
  assert.equal(restored.signal.aborted, false)
  assert.notEqual(restored.signal, before.signal)
})

test('an initial IndexedDB read cannot resurrect a key after logout', async () => {
  const original = await auth.generateAuthKey()
  const service = keys(undefined)
  service.cached = undefined
  let read
  service.idb = mode =>
    mode === 'readonly'
      ? new Promise(resolve => {
          read = resolve
        })
      : Promise.resolve()
  const loading = service.get()
  await service.clear()
  read(original)
  assert.equal(await loading, null)
  assert.equal(await service.lease(), null)
})

test('a failed initial read cannot erase a replacement identity', async () => {
  const service = keys()
  service.cached = undefined
  let fail
  service.idb = mode =>
    mode === 'readonly'
      ? new Promise((_, reject) => {
          fail = reject
        })
      : Promise.resolve()
  const loading = service.get()
  const replacement = await auth.generateAuthKey()
  await service.put(replacement)
  fail(new Error('old read failed'))
  assert.equal(await loading, replacement)
  assert.equal((await service.lease()).key, replacement)
})
