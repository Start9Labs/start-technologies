import assert from 'node:assert/strict'
import test from 'node:test'
import { indexedEntries } from '../src/util/indexed-entries.ts'

test('entries sort by index, then unindexed entries by key', () => {
  const record = {
    z: { index: 0 },
    b: {},
    a: { index: 2 },
    c: { index: null },
    m: { index: 1 },
    gone: undefined,
  }
  assert.deepEqual(
    indexedEntries(record).map(([key]) => key),
    ['z', 'm', 'a', 'b', 'c'],
  )
})

test('key order of the record does not matter', () => {
  const forward = { a: { index: 1 }, b: { index: 1 }, c: {} }
  const reversed = { c: {}, b: { index: 1 }, a: { index: 1 } }
  assert.deepEqual(
    indexedEntries(forward).map(([key]) => key),
    indexedEntries(reversed).map(([key]) => key),
  )
})
