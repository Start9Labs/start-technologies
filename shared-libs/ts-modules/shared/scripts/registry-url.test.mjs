import assert from 'node:assert/strict'
import test from 'node:test'
import { registryUrl } from '../src/util/misc.util.ts'

test('registry URLs preserve their base path and normalize the origin and trailing slash', () => {
  assert.equal(
    registryUrl(' Registry.Example/catalog/ '),
    'https://registry.example/catalog/',
  )
  assert.equal(registryUrl('registry.example'), 'https://registry.example/')
  assert.equal(registryUrl('example.onion'), 'http://example.onion/')
  assert.equal(
    registryUrl('https://example.onion/catalog'),
    'https://example.onion/catalog/',
  )
  assert.equal(
    registryUrl('http://registry.example:8080/catalog/'),
    'http://registry.example:8080/catalog/',
  )
  assert.equal(
    new URL('rpc/v0', registryUrl('registry.example/catalog')).href,
    'https://registry.example/catalog/rpc/v0',
  )
})

test('registry URLs reject credentials, queries, fragments and non-HTTP transports', () => {
  for (const url of [
    'ftp://registry.example',
    'https://user:password@registry.example',
    'https://registry.example?query=1',
    'https://registry.example/#fragment',
  ]) {
    assert.throws(() => registryUrl(url))
  }
})
