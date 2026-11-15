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

async function peer(lease) {
  const body = new TextEncoder().encode('captured body')
  return auth.verifyRequestProof(
    await lease.signer.sign('server.test', body),
    'server.test',
    body,
  )
}

test('possession admission shares an unpersisted key without changing enrollment state', async () => {
  const service = keys()
  let writes = 0
  service.idb = async mode => {
    if (mode === 'readwrite') writes++
  }
  const [first, second] = await Promise.all([
    service.admissionLease(),
    service.admissionLease(),
  ])
  assert.equal(first.signer, second.signer)
  assert.equal(first.signal, second.signal)
  assert.deepEqual(Object.keys(first.signer), ['sign'])
  assert.equal('key' in first, false)
  assert.equal(await peer(first), await peer(second))
  assert.equal(await service.lease(), null)
  assert.equal(writes, 0)
  await service.clear()
  assert.equal(first.signal.aborted, true)
  const fresh = await service.admissionLease()
  assert.notEqual(fresh.signer, first.signer)
  assert.equal(fresh.signal.aborted, false)
  assert.equal(writes, 1)
})

test('enrollment and rollback invalidate their displaced possession leases', async () => {
  const service = keys()
  const before = await service.admissionLease()
  const replacement = await service.create()
  const active = await service.admissionLease()
  assert.equal(before.signal.aborted, true)
  assert.equal(await peer(active), auth.normalizeDer64(replacement.pubkeyPem))
  assert.equal(active.signal, (await service.lease()).signal)
  assert.equal(active.key, replacement)
  await service.rollback()
  assert.equal(active.signal.aborted, true)
  assert.equal(await service.lease(), null)
  const restored = await service.admissionLease()
  assert.notEqual(restored.signer, before.signer)
  assert.equal(restored.signal.aborted, false)
})

test('logout cancels pending admission lookup instead of reviving it with a fresh identity', async () => {
  const original = await auth.generateAuthKey()
  const service = keys()
  service.cached = undefined
  const read = Promise.withResolvers()
  service.idb = mode => (mode === 'readonly' ? read.promise : Promise.resolve())
  const loading = service.admissionLease()
  const rejected = assert.rejects(loading, { name: 'AbortError' })
  await service.clear()
  read.resolve(original)
  await rejected
  assert.equal(await service.lease(), null)
  assert.notEqual(
    await peer(await service.admissionLease()),
    auth.normalizeDer64(original.pubkeyPem),
  )
})

test('replacement cancels pending possession-key generation without adopting its late result', async () => {
  const service = keys()
  const generated = Promise.withResolvers()
  service.admissionKey = generated.promise
  const loading = service.admissionLease()
  const rejected = assert.rejects(loading, { name: 'AbortError' })
  await Promise.resolve()
  const replacement = await auth.generateAuthKey()
  await service.put(replacement)
  generated.resolve(await auth.generateEphemeralRequestSigner())
  await rejected
  assert.equal(
    await peer(await service.admissionLease()),
    auth.normalizeDer64(replacement.pubkeyPem),
  )
})

test('an enrolled lease lookup cannot adopt a replacement identity after cancellation', async () => {
  const service = keys()
  service.cached = undefined
  const read = Promise.withResolvers()
  service.idb = mode => (mode === 'readonly' ? read.promise : Promise.resolve())
  const loading = service.lease()
  const rejected = assert.rejects(loading, { name: 'AbortError' })
  const replacement = await auth.generateAuthKey()
  await service.put(replacement)
  read.resolve(await auth.generateAuthKey())
  await rejected
  assert.equal((await service.lease()).key, replacement)
})

test('logout cancels pending enrollment key generation before it can persist or revive an identity', async () => {
  const service = keys()
  let writes = 0
  service.idb = async mode => {
    if (mode === 'readwrite') writes++
  }
  const creating = service.create()
  const rejected = assert.rejects(creating, { name: 'AbortError' })
  await service.clear()
  await rejected
  assert.equal(await service.lease(), null)
  assert.equal(writes, 1)
})

test('logout cancels enrollment while its persistence is pending', async () => {
  const service = keys()
  const started = Promise.withResolvers()
  const persisted = Promise.withResolvers()
  let writes = 0
  service.idb = mode => {
    if (mode !== 'readwrite') return Promise.resolve()
    if (++writes === 1) {
      started.resolve()
      return persisted.promise
    }
    return Promise.resolve()
  }
  const creating = service.create()
  const rejected = assert.rejects(creating, { name: 'AbortError' })
  await started.promise
  await service.clear()
  persisted.resolve()
  await rejected
  assert.equal(await service.lease(), null)
})

test('logout cancels a pending HTTP resource proof without returning a stale signed header', async context => {
  const service = keys(await auth.generateAuthKey())
  const original = crypto.subtle.sign.bind(crypto.subtle)
  const started = Promise.withResolvers()
  const finish = Promise.withResolvers()
  context.mock.method(crypto.subtle, 'sign', async (...args) => {
    started.resolve()
    await finish.promise
    return original(...args)
  })
  const signing = service.signHeader('captured resource')
  const rejected = assert.rejects(signing, { name: 'AbortError' })
  await started.promise
  await service.clear()
  finish.resolve()
  await rejected
})

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
