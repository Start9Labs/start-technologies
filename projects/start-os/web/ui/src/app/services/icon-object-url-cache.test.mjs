import assert from 'node:assert/strict'
import test from 'node:test'
import { firstValueFrom } from 'rxjs'

import { IconObjectUrlCache } from './icon-object-url-cache.ts'
import {
  FALLBACK_ICON,
  registryIconUrl,
} from '../../../../../../../shared-libs/ts-modules/marketplace/src/util/icon.ts'

function pending() {
  let resolve
  const promise = new Promise(done => {
    resolve = done
  })
  return { promise, resolve }
}

test('registry icon URLs preserve prefixes and encode package and dependency paths', () => {
  assert.equal(
    registryIconUrl(
      'http://private.onion/store/',
      'pkg/a',
      '1.2:flavor',
      'dep?#',
    ),
    'http://private.onion/store/icons/pkg%2Fa/1.2%3Aflavor/dependencies/dep%3F%23',
  )
  assert.equal(
    registryIconUrl('http://192.168.1.2:8080/', 'pkg', '1'),
    'http://192.168.1.2:8080/icons/pkg/1',
  )
})

test('object URL requests are lazy, shared and retained after completion', async () => {
  const response = pending()
  const calls = []
  const cache = new IconObjectUrlCache(url => {
    calls.push(url)
    return response.promise
  }, FALLBACK_ICON)
  const icon = cache.get('registry-key', '/registry/icons/encoded')
  assert.equal(icon, cache.get('registry-key', '/registry/icons/encoded'))
  assert.deepEqual(calls, [])

  const first = firstValueFrom(icon)
  const second = firstValueFrom(icon)
  assert.deepEqual(calls, ['/registry/icons/encoded'])
  response.resolve('blob:registry')
  assert.deepEqual(await Promise.all([first, second]), [
    'blob:registry',
    'blob:registry',
  ])
  assert.equal(
    await firstValueFrom(cache.get('registry-key', '/registry/icons/encoded')),
    'blob:registry',
  )
  assert.equal(calls.length, 1)
})

test('failed object URL requests emit fallback and allow retry', async () => {
  let calls = 0
  const cache = new IconObjectUrlCache(async () => {
    if (++calls === 1) throw new Error('unavailable')
    return 'blob:retry'
  }, FALLBACK_ICON)
  assert.equal(
    await firstValueFrom(cache.get('key', '/registry/icons/encoded')),
    FALLBACK_ICON,
  )
  assert.equal(
    await firstValueFrom(cache.get('key', '/registry/icons/encoded')),
    'blob:retry',
  )
  assert.equal(calls, 2)
})

test('installed archive identities do not reuse an older icon', async () => {
  let calls = 0
  const cache = new IconObjectUrlCache(
    async () => `blob:${++calls}`,
    FALLBACK_ICON,
  )
  const path = '/s9pk/installed/pkg.s9pk/icon'
  assert.equal(await firstValueFrom(cache.get('old.s9pk/icon', path)), 'blob:1')
  assert.equal(await firstValueFrom(cache.get('new.s9pk/icon', path)), 'blob:2')
  assert.equal(await firstValueFrom(cache.get('old.s9pk/icon', path)), 'blob:1')
  assert.equal(calls, 2)
})
