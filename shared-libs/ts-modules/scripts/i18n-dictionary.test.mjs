import assert from 'node:assert/strict'
import test from 'node:test'
import { dictionaryEntries } from './i18n-dictionary.mjs'

test('dictionary keys follow TypeScript quoting and escapes', () => {
  const entries = dictionaryEntries(String.raw`export const ENGLISH = {
    'Simple': 1,
    "isn't trusted": 2,
    'Escaped \'quote\'': 3,
    'Unicode \u2014 dash': 4,
  }`)
  assert.deepEqual(
    [...entries],
    [
      ['Simple', 1],
      ["isn't trusted", 2],
      ["Escaped 'quote'", 3],
      ['Unicode — dash', 4],
    ],
  )
})

test('translated dictionary entries include numeric IDs and decode text literals', () => {
  assert.deepEqual(
    [
      ...dictionaryEntries(
        `export default { 2: "L'utilisateur", 3: 'Texte' } satisfies i18n`,
      ),
    ],
    [
      ['2', "L'utilisateur"],
      ['3', 'Texte'],
    ],
  )
})
