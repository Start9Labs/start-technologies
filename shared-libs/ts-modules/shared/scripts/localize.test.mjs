import assert from 'node:assert/strict'
import test from 'node:test'
import { localizeFor } from '../src/i18n/localize.ts'

test('translations resolve exact, then same-language, then the declared default', () => {
  const text = { de_DE: 'Deutsch', en_GB: 'English', pl_PL: 'Polski' }
  assert.equal(localizeFor(text, 'pl_PL', 'de_DE'), 'Polski')
  assert.equal(localizeFor(text, 'en_US', 'de_DE'), 'English')
  assert.equal(localizeFor(text, 'fr_FR', 'pl_PL'), 'Polski')
  assert.equal(localizeFor(text, 'fr_FR', 'de_DE'), 'Deutsch')
  assert.ok(Object.values(text).includes(localizeFor(text, 'fr_FR', 'ja_JP')))
  assert.ok(Object.values(text).includes(localizeFor(text, 'fr_FR')))
  assert.equal(localizeFor({}, 'en_US', 'de_DE'), '')
  assert.equal(localizeFor('Plain', 'pl_PL', 'de_DE'), 'Plain')
})
