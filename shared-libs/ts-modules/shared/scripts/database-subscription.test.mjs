import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { Subject } from 'rxjs'
import { TestScheduler } from 'rxjs/testing'
import './typescript-loader.mjs'
const {
  databaseSubscription,
  legacyDatabaseSubscription,
  bufferDatabaseRevisions,
} = await import('../src/services/database-subscription.ts')
const flush = () => new Promise(setImmediate)

test('disposing while a key lease is pending never starts a database stream', async () => {
  const lease = Promise.withResolvers()
  let streams = 0
  const subscription = databaseSubscription(
    {
      stream: () => {
        streams++
        return new Subject()
      },
    },
    { lease: () => lease.promise },
    { openObserver: { next() {} } },
  ).subscribe()
  subscription.unsubscribe()
  lease.resolve({ key: {}, signal: new AbortController().signal })
  await flush()
  assert.equal(streams, 0)
})

test('legacy database ownership starts with its dump and disposes its revision subscription', async () => {
  const revisions = new Subject()
  const initial = { id: 4, value: { name: 'cached' } }
  const changes = []
  let requests = 0
  const updates = legacyDatabaseSubscription(
    async () => {
      requests++
      return { dump: initial, guid: 'legacy' }
    },
    guid => {
      assert.equal(guid, 'legacy')
      return revisions
    },
  )
  assert.equal(requests, 0)
  const subscription = updates.subscribe(value => changes.push(value))
  await flush()
  assert.deepEqual(changes, [initial])
  const revision = { id: 5, patch: [] }
  revisions.next(revision)
  assert.deepEqual(changes, [initial, revision])
  subscription.unsubscribe()
  assert.equal(revisions.observers.length, 0)
})

test('disposing a pending legacy initialization never opens its GUID socket', async () => {
  const pending = Promise.withResolvers()
  let opened = 0
  const subscription = legacyDatabaseSubscription(
    () => pending.promise,
    () => {
      opened++
      return new Subject()
    },
  ).subscribe()
  subscription.unsubscribe()
  pending.resolve({ dump: { id: 0, value: {} }, guid: 'unused' })
  await flush()
  assert.equal(opened, 0)
})

test('database dumps are immediate, revisions batch at 250ms, and disposal discards later work', () => {
  new TestScheduler((actual, expected) =>
    assert.deepEqual(actual, expected),
  ).run(({ cold, expectObservable }) => {
    const dump = { id: 10, value: {} }
    const first = { id: 11, patch: [] }
    const second = { id: 12, patch: [] }
    const later = { id: 13, patch: [] }
    const updates = cold('a 9ms b 9ms c 279ms d', {
      a: dump,
      b: first,
      c: second,
      d: later,
    })
    expectObservable(updates.pipe(bufferDatabaseRevisions()), '^ 259ms !').toBe(
      'a 249ms b',
      { a: [dump], b: [first, second] },
    )
  })
})

test('the revision batching window starts at the initiating dump, not during admission', () => {
  new TestScheduler((actual, expected) =>
    assert.deepEqual(actual, expected),
  ).run(({ cold, expectObservable }) => {
    const dump = { id: 10, value: {} }
    const revision = { id: 11, patch: [] }
    expectObservable(
      cold('100ms a 9ms b', { a: dump, b: revision }).pipe(
        bufferDatabaseRevisions(),
      ),
      '^ 359ms !',
    ).toBe('100ms a 249ms b', { a: [dump], b: [revision] })
  })
})

test('transport loss discards pending revisions rather than publishing a stale batch', () => {
  new TestScheduler((actual, expected) =>
    assert.deepEqual(actual, expected),
  ).run(({ cold, expectObservable }) => {
    const dump = { id: 10, value: {} }
    const revision = { id: 11, patch: [] }
    expectObservable(
      cold('a 9ms b 9ms #', { a: dump, b: revision }).pipe(
        bufferDatabaseRevisions(),
      ),
    ).toBe('a 19ms #', { a: [dump] })
  })
})
