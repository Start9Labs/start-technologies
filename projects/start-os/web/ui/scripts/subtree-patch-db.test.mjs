import assert from 'node:assert/strict'
import test from 'node:test'
import { BehaviorSubject, Observable, Subject } from 'rxjs'
import {
  applyOperation,
  extension,
  minimalSelection,
  NOTHING,
  normalizeSelector,
  PatchDB,
  projectNarrowed,
  projectOperations,
  projectRoot,
  projectScoped,
  reconcile,
  reprojectRoot,
  scopeOperations,
  selectionContains,
  SubtreePatchDB,
  watchSelectors,
} from 'patch-db-client'

test('subtree patch scoping preserves server removal and missing-value semantics', () => {
  for (const pointer of ['/a', '/a/b']) {
    assert.deepEqual(scopeOperations([{ op: 'remove', path: '/a' }], pointer), [
      { op: 'replace', path: '', value: null },
    ])
  }
  for (const op of ['add', 'replace']) {
    assert.deepEqual(
      scopeOperations([{ op, path: '/a', value: { b: 1 } }], '/a/b'),
      [{ op, path: '', value: 1 }],
    )
    assert.deepEqual(scopeOperations([{ op, path: '/a', value: {} }], '/a/b'), [
      { op, path: '', value: null },
    ])
  }
  assert.deepEqual(scopeOperations([{ op: 'remove', path: '/a/b' }], '/a'), [
    { op: 'remove', path: '/b' },
  ])
  assert.deepEqual(scopeOperations([{ op: 'remove', path: '/a' }], ''), [
    { op: 'remove', path: '/a' },
  ])
  assert.deepEqual(scopeOperations([{ op: 'remove', path: '/ab' }], '/a'), [])
})

const shape = selection => selection.map(normalizeSelector)
const sorted = selection =>
  [...selection].sort((a, b) => (a.include < b.include ? -1 : 1))

function fixture(
  t,
  { releaseDelay = 5000, update = () => Promise.resolve() } = {},
) {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const calls = []
  const updates = []
  const source = {
    subscribe(path, since, selection) {
      const events = new Subject()
      const call = {
        path,
        since,
        selection: shape(selection),
        events,
        closed: false,
      }
      calls.push(call)
      return new Observable(subscriber => {
        const subscription = events.subscribe(subscriber)
        return () => {
          call.closed = true
          subscription.unsubscribe()
        }
      })
    },
    update(subscription, selection) {
      updates.push({ subscription, selection: shape(selection) })
      return update()
    },
  }
  const cache = new BehaviorSubject({ id: 0, value: {} })
  const db = new SubtreePatchDB(source, cache, { releaseDelay, batch: 250 })
  const start = (call, id, value, epoch = 'boot') =>
    call.events.next({ epoch, id, value, subscription: 'sub' })
  const revision = (call, id, patch) => call.events.next({ id, patch })
  const extend = (call, id, patch = []) =>
    call.events.next({ id, patch, extend: true })
  const observe = observable => {
    const values = []
    const subscription = observable.subscribe(v => values.push(v))
    t.after(() => subscription.unsubscribe())
    return { values, subscription }
  }
  const watch = (...path) => observe(db.watch$(...path))
  const project = (projection, ...path) =>
    observe(db.watchProjection$(projection, ...path))
  t.after(() => db.stop())
  return {
    db,
    cache,
    calls,
    updates,
    start,
    revision,
    extend,
    watch,
    project,
  }
}

test('watches share one subscription selecting their union', t => {
  const f = fixture(t)
  const info = f.watch('serverInfo')
  const again = f.watch('serverInfo')
  const packages = f.watch('packageData')
  f.db.start()
  assert.equal(f.calls.length, 1)
  assert.deepEqual(f.calls[0].selection, [
    { include: '/packageData' },
    { include: '/serverInfo' },
  ])
  assert.equal(f.calls[0].since, null)
  f.start(f.calls[0], 3, { serverInfo: { name: 'box' }, packageData: {} })
  assert.deepEqual(info.values, [{ name: 'box' }])
  assert.deepEqual(again.values, [{ name: 'box' }])
  assert.deepEqual(packages.values, [{}])
  assert.equal(f.cache.value.id, 3)
  assert.deepEqual(f.updates, [])
})

test('a new watch widens the selection and is served once its extension arrives', t => {
  const f = fixture(t)
  f.watch('serverInfo')
  f.db.start()
  f.start(f.calls[0], 3, { serverInfo: { name: 'box' } })
  const ui = f.watch('ui')
  assert.deepEqual(f.updates, [
    {
      subscription: 'sub',
      selection: [{ include: '/serverInfo' }, { include: '/ui' }],
    },
  ])
  assert.deepEqual(ui.values, [])
  f.extend(f.calls[0], 3, [{ op: 'add', path: '/ui', value: { theme: 1 } }])
  assert.deepEqual(ui.values, [{ theme: 1 }])
  assert.deepEqual(f.cache.value.value, {
    serverInfo: { name: 'box' },
    ui: { theme: 1 },
  })
  assert.equal(f.calls.length, 1)
})

test('a released watch narrows the selection after the delay and prunes its data', t => {
  const f = fixture(t)
  f.watch('serverInfo')
  const ui = f.watch('ui')
  f.db.start()
  f.start(f.calls[0], 3, { serverInfo: { name: 'box' }, ui: { theme: 1 } })
  ui.subscription.unsubscribe()
  t.mock.timers.tick(4999)
  assert.deepEqual(f.updates, [])
  const again = f.watch('ui')
  t.mock.timers.tick(10_000)
  assert.deepEqual(f.updates, [])
  assert.deepEqual(again.values, [{ theme: 1 }])
  again.subscription.unsubscribe()
  t.mock.timers.tick(5000)
  assert.deepEqual(f.updates.at(-1).selection, [{ include: '/serverInfo' }])
  assert.deepEqual(f.cache.value.value, { serverInfo: { name: 'box' } })
  f.revision(f.calls[0], 4, [{ op: 'replace', path: '/ui/theme', value: 2 }])
  f.extend(f.calls[0], 4)
  assert.deepEqual(f.cache.value.value, { serverInfo: { name: 'box' } })
  assert.equal(f.cache.value.id, 4)
})

test('widening keeps the document and notifies no existing watch', t => {
  const f = fixture(t)
  const info = f.watch('serverInfo')
  const listing = f.project({ include: ['/*/title'] }, 'packages')
  f.db.start()
  f.start(f.calls[0], 3, {
    serverInfo: { name: 'box' },
    packages: { a: { title: 'A', notes: 'n' } },
  })
  const before = f.cache.value.value
  f.project({ include: ['/a/notes'] }, 'packages')
  assert.equal(f.cache.value.value, before)
  assert.equal(info.values.length, 1)
  assert.deepEqual(listing.values, [{ a: { title: 'A' } }])
})

test('narrowing notifies only the watches whose subtree it prunes', t => {
  const f = fixture(t)
  const info = f.watch('serverInfo')
  const listing = f.project({ include: ['/*/title'] }, 'packages')
  const notes = f.project({ include: ['/a/notes'] }, 'packages')
  f.db.start()
  f.start(f.calls[0], 3, {
    serverInfo: { name: 'box' },
    packages: { a: { title: 'A', notes: 'n' } },
  })
  const serverInfo = f.cache.value.value.serverInfo
  notes.subscription.unsubscribe()
  t.mock.timers.tick(5000)
  assert.deepEqual(f.cache.value.value, {
    serverInfo: { name: 'box' },
    packages: { a: { title: 'A' } },
  })
  assert.equal(f.cache.value.value.serverInfo, serverInfo)
  assert.equal(info.values.length, 1)
  assert.deepEqual(listing.values.at(-1), { a: { title: 'A' } })
})

test('narrowing a whole-document watch retains only confirmed subset coverage', t => {
  const f = fixture(t)
  const info = f.watch('serverInfo')
  const whole = f.watch()
  f.db.start()
  f.start(f.calls[0], 3, { serverInfo: { name: 'box' }, ui: { theme: 1 } })
  whole.subscription.unsubscribe()
  t.mock.timers.tick(5000)
  const ui = f.watch('ui')
  assert.deepEqual(ui.values, [])
  assert.deepEqual(f.cache.value.value, { serverInfo: { name: 'box' } })
  assert.deepEqual(info.values.at(-1), { name: 'box' })
  f.calls[0].events.error(new Error('closed before confirmation'))
  assert.deepEqual(f.cache.value.value, { serverInfo: { name: 'box' } })
  f.db.start()
  assert.deepEqual(f.calls[1].since, { epoch: 'boot', id: 3 })
  assert.deepEqual(f.calls[1].selection, [{ include: '/serverInfo' }])
  f.calls[1].events.next({
    epoch: 'boot',
    id: 3,
    subscription: 'again',
    revisions: [],
  })
  assert.deepEqual(ui.values, [])
  f.extend(f.calls[1], 3, [{ op: 'add', path: '/ui', value: { theme: 2 } }])
  assert.deepEqual(ui.values, [{ theme: 2 }])
})

test('releasing every watch resumes an empty replica with NOTHING, not universal coverage', t => {
  const f = fixture(t)
  const ui = f.watch('ui')
  f.db.start()
  f.start(f.calls[0], 3, { ui: { theme: 1 } })
  ui.subscription.unsubscribe()
  t.mock.timers.tick(5000)
  const info = f.watch('serverInfo')
  assert.deepEqual(info.values, [])
  f.calls[0].events.error(new Error('closed before confirmation'))
  f.db.start()
  assert.deepEqual(f.calls[1].selection, shape(NOTHING))
  assert.deepEqual(f.calls[1].since, { epoch: 'boot', id: 3 })
  assert.deepEqual(info.values, [])
})

for (const { name, patch, expected } of [
  {
    name: 'insertion',
    patch: [{ op: 'add', path: '/arr/0', value: 'x' }],
    expected: [null, 'a', null, null],
  },
  {
    name: 'removal',
    patch: [{ op: 'remove', path: '/arr/0' }],
    expected: [null, 'c'],
  },
]) {
  test(`narrowing preserves a selected array position through an in-flight ${name}`, t => {
    const f = fixture(t)
    const whole = f.watch('arr')
    const second = f.watch('arr', 1)
    f.db.start()
    f.start(f.calls[0], 3, { arr: ['a', 'b', 'c'] })
    whole.subscription.unsubscribe()
    t.mock.timers.tick(5000)
    assert.deepEqual(f.updates.at(-1).selection, [{ include: '/arr/1' }])
    assert.deepEqual(f.cache.value.value, { arr: [null, 'b', null] })
    f.revision(f.calls[0], 4, patch)
    t.mock.timers.tick(250)
    assert.deepEqual(f.cache.value.value, { arr: expected })
    assert.equal(second.values.at(-1), expected[1])
    f.extend(f.calls[0], 4)
    assert.deepEqual(f.cache.value.value, { arr: expected })
    assert.equal(f.cache.value.id, 4)
    f.revision(f.calls[0], 5, [
      { op: 'replace', path: '/arr/1', value: 'next' },
    ])
    t.mock.timers.tick(250)
    assert.equal(second.values.at(-1), 'next')
  })
}

test('failure before narrowing confirmation resumes the correctly shifted array', t => {
  const f = fixture(t)
  const whole = f.watch('arr')
  const second = f.watch('arr', 1)
  f.db.start()
  f.start(f.calls[0], 3, { arr: ['a', 'b', 'c'] })
  whole.subscription.unsubscribe()
  t.mock.timers.tick(5000)
  f.revision(f.calls[0], 4, [{ op: 'add', path: '/arr/0', value: 'x' }])
  f.calls[0].events.error(new Error('closed before confirmation'))
  assert.deepEqual(f.cache.value.value, { arr: [null, 'a', null, null] })
  assert.equal(second.values.at(-1), 'a')
  f.db.start()
  assert.deepEqual(f.calls[1].since, { epoch: 'boot', id: 4 })
  assert.deepEqual(f.calls[1].selection, [{ include: '/arr/1' }])
  f.calls[1].events.next({
    epoch: 'boot',
    id: 4,
    subscription: 'again',
    revisions: [],
  })
  f.revision(f.calls[1], 5, [
    { op: 'replace', path: '/arr', value: [null, 'later', null] },
  ])
  t.mock.timers.tick(250)
  assert.deepEqual(f.cache.value.value, { arr: [null, 'later', null] })
  assert.equal(second.values.at(-1), 'later')
})

test('a watch acquired while flushing revisions cannot start a second selection update', t => {
  const f = fixture(t)
  let requested = false
  let third
  const first = f.db.watch$('a').subscribe(value => {
    if (value.n !== 1 || requested) return
    requested = true
    third = f.watch('c')
  })
  t.after(() => first.unsubscribe())
  f.db.start()
  f.start(f.calls[0], 3, { a: { n: 0 } })
  f.revision(f.calls[0], 4, [{ op: 'replace', path: '/a/n', value: 1 }])
  const second = f.watch('b')
  assert.equal(f.updates.length, 1)
  assert.deepEqual(f.updates[0].selection, [
    { include: '/a' },
    { include: '/b' },
    { include: '/c' },
  ])
  assert.deepEqual(second.values, [])
  assert.deepEqual(third.values, [])
  f.extend(f.calls[0], 4, [
    { op: 'add', path: '/b', value: 2 },
    { op: 'add', path: '/c', value: 3 },
  ])
  assert.deepEqual(second.values, [2])
  assert.deepEqual(third.values, [3])
  assert.equal(f.updates.length, 1)
})

test('selection changes wait for the previous extension', t => {
  const f = fixture(t)
  f.watch('serverInfo')
  f.db.start()
  f.start(f.calls[0], 3, { serverInfo: {} })
  const ui = f.watch('ui')
  const packages = f.watch('packageData')
  assert.equal(f.updates.length, 1)
  f.extend(f.calls[0], 3, [{ op: 'add', path: '/ui', value: {} }])
  assert.deepEqual(ui.values.at(-1), {})
  assert.deepEqual(packages.values, [])
  assert.equal(f.updates.length, 2)
  assert.deepEqual(f.updates[1].selection, [
    { include: '/packageData' },
    { include: '/serverInfo' },
    { include: '/ui' },
  ])
  f.extend(f.calls[0], 3, [{ op: 'add', path: '/packageData', value: {} }])
  assert.deepEqual(packages.values.at(-1), {})
})

test('revisions apply in order and before an extension', t => {
  const f = fixture(t)
  const info = f.watch('serverInfo')
  f.db.start()
  f.start(f.calls[0], 3, { serverInfo: { n: 0 } })
  f.revision(f.calls[0], 4, [
    { op: 'replace', path: '/serverInfo/n', value: 1 },
  ])
  f.watch('ui')
  f.revision(f.calls[0], 5, [
    { op: 'replace', path: '/serverInfo/n', value: 2 },
  ])
  f.extend(f.calls[0], 5, [{ op: 'add', path: '/ui', value: {} }])
  assert.deepEqual(info.values.at(-1), { n: 2 })
  assert.equal(f.cache.value.id, 5)
})

test('watchers observe the complete package-state transition', t => {
  const f = fixture(t)
  const watched = f.watch('packageData', 'x')
  f.db.start()
  const before = {
    stateInfo: {
      state: 'updating',
      manifest: { id: 'x' },
      installingInfo: { newManifest: { id: 'x' } },
    },
  }
  const after = { stateInfo: { state: 'installed', manifest: { id: 'x' } } }
  f.start(f.calls[0], 1, { packageData: { x: before } })
  f.revision(f.calls[0], 2, [
    { op: 'remove', path: '/packageData/x/stateInfo/installingInfo' },
    {
      op: 'replace',
      path: '/packageData/x/stateInfo/state',
      value: 'installed',
    },
  ])
  t.mock.timers.tick(250)
  assert.deepEqual(watched.values, [before, after])
})

test('a projected watch selects and reads only its projection', t => {
  const f = fixture(t)
  const listing = f.project({ exclude: ['/*/icon'] }, 'packageData')
  f.db.start()
  assert.deepEqual(f.calls[0].selection, [
    { include: '/packageData', exclude: ['/packageData/*/icon'] },
  ])
  f.start(f.calls[0], 3, { packageData: { x: { state: 1 } } })
  assert.deepEqual(listing.values, [{ x: { state: 1 } }])
  const full = f.watch('packageData', 'x')
  assert.deepEqual(
    sorted(f.updates[0].selection),
    sorted([
      { include: '/packageData', exclude: ['/packageData/*/icon'] },
      { include: '/packageData/x' },
    ]),
  )
  f.extend(f.calls[0], 3, [
    { op: 'add', path: '/packageData/x/icon', value: 'big' },
  ])
  assert.deepEqual(full.values, [{ state: 1, icon: 'big' }])
  assert.deepEqual(listing.values.at(-1), { x: { state: 1 } })
})

test('a contained watch adds nothing to the selection', t => {
  const f = fixture(t)
  f.watch('packageData')
  f.project({ include: ['/*/state'] }, 'packageData')
  f.watch('packageData', 'x')
  f.db.start()
  assert.deepEqual(f.calls[0].selection, [{ include: '/packageData' }])
})

test('a failure keeps the data and resumes from the cursor with the confirmed selection', t => {
  const f = fixture(t)
  const info = f.watch('serverInfo')
  const failures = []
  f.db.failures$.subscribe(e => failures.push(e))
  f.db.start()
  f.start(f.calls[0], 5, { serverInfo: { n: 1 } })
  f.watch('ui')
  f.calls[0].events.error(new Error('closed'))
  assert.equal(failures.length, 1)
  assert.deepEqual(info.values.at(-1), { n: 1 })
  f.db.start()
  assert.deepEqual(f.calls[1].since, { epoch: 'boot', id: 5 })
  assert.deepEqual(f.calls[1].selection, [{ include: '/serverInfo' }])
  f.calls[1].events.next({
    epoch: 'boot',
    id: 6,
    subscription: 'again',
    revisions: [
      { id: 6, patch: [{ op: 'replace', path: '/serverInfo/n', value: 2 }] },
    ],
  })
  assert.deepEqual(info.values.at(-1), { n: 2 })
  assert.equal(f.updates.at(-1).subscription, 'again')
  assert.deepEqual(f.updates.at(-1).selection, [
    { include: '/serverInfo' },
    { include: '/ui' },
  ])
})

test('a late selection-update failure cannot stop a replacement subscription', async t => {
  let rejectOld
  const oldReply = new Promise((_, reject) => {
    rejectOld = reject
  })
  let requests = 0
  const f = fixture(t, {
    update: () => (requests++ ? Promise.resolve() : oldReply),
  })
  const failures = []
  f.db.failures$.subscribe(error => failures.push(error))
  f.watch('serverInfo')
  f.db.start()
  f.start(f.calls[0], 3, { serverInfo: { name: 'box' } })
  f.watch('ui')
  f.calls[0].events.error(new Error('stream lost'))
  f.db.start()
  f.calls[1].events.next({
    epoch: 'boot',
    id: 3,
    subscription: 'again',
    revisions: [],
  })
  rejectOld(new Error('old update failed'))
  await Promise.resolve()
  assert.equal(failures.length, 1)
  assert.equal(f.calls[1].closed, false)
  f.extend(f.calls[1], 3, [{ op: 'add', path: '/ui', value: {} }])
  assert.deepEqual(f.cache.value.value, { serverInfo: { name: 'box' }, ui: {} })
})

test('a restored snapshot serves its selection offline and resumes it from its cursor', t => {
  const f = fixture(t)
  const cursor = { epoch: 'boot', id: 4 }
  f.db.restore(
    { id: 4, value: { serverInfo: { n: 1 }, packageData: {} } },
    { '/serverInfo': cursor, '/packageData': cursor },
  )
  const info = f.watch('serverInfo')
  assert.deepEqual(info.values, [{ n: 1 }])
  assert.deepEqual(f.db.snapshotCursors(), {
    '/packageData': cursor,
    '/serverInfo': cursor,
  })
  f.db.start()
  assert.deepEqual(f.calls[0].since, cursor)
  assert.deepEqual(f.calls[0].selection, [
    { include: '/packageData' },
    { include: '/serverInfo' },
  ])
  f.calls[0].events.next({
    epoch: 'boot',
    id: 4,
    subscription: 'sub',
    revisions: [],
  })
  assert.deepEqual(f.updates[0].selection, [{ include: '/serverInfo' }])
})

test('a snapshot whose cursors disagree resumes from a dump', t => {
  const f = fixture(t)
  f.db.restore(
    { id: 4, value: { serverInfo: {} } },
    {
      '/serverInfo': { epoch: 'boot', id: 4 },
      '/ui': { epoch: 'boot', id: 3 },
    },
  )
  f.watch('serverInfo')
  f.db.start()
  assert.equal(f.calls[0].since, null)
})

test('numeric watch paths select that key or position', () => {
  assert.deepEqual(watchSelectors('/bindings/80'), [
    { include: '/bindings/80' },
  ])
  assert.deepEqual(
    watchSelectors(
      JSON.stringify(['/rows', { include: ['/*/a'], exclude: ['/0'] }]),
    ),
    [{ include: '/rows/*/a', exclude: ['/rows/0'] }],
  )
  assert.deepEqual(
    projectRoot([{ include: '/arr/1' }], { arr: ['a', 'b', 'c'] }),
    { arr: [null, 'b', null] },
  )
})

const arrayDocument = () => ({
  rows: [
    { a: 1, b: 10 },
    { a: 2, b: 20 },
    { a: 3, b: 30 },
  ],
})

for (const { name, patch, expected } of [
  {
    name: 'an insertion',
    patch: [{ op: 'add', path: '/rows/0', value: { a: 4, b: 40 } }],
    expected: [4, 1, 2, 3],
  },
  {
    name: 'a removal',
    patch: [{ op: 'remove', path: '/rows/0' }],
    expected: [2, 3],
  },
  {
    name: 'a prepend window shift',
    patch: [
      { op: 'add', path: '/rows/0', value: { a: 4, b: 40 } },
      { op: 'remove', path: '/rows/3' },
    ],
    expected: [4, 1, 2],
  },
  {
    name: 'an append window shift',
    patch: [
      { op: 'remove', path: '/rows/0' },
      { op: 'add', path: '/rows/2', value: { a: 4, b: 40 } },
    ],
    expected: [2, 3, 4],
  },
]) {
  test(`ordinary positional watches follow ${name}`, t => {
    const source = new Subject()
    const cache = new BehaviorSubject({ id: 3, value: arrayDocument() })
    const db = new PatchDB(source, cache)
    const values = []
    const subscription = db
      .watch$('rows', 1, 'a')
      .subscribe(value => values.push(value))
    t.after(() => {
      subscription.unsubscribe()
      db.stop()
    })
    db.start()
    source.next([{ id: 4, patch }])
    assert.deepEqual(cache.value.value.rows.map(row => row.a), expected)
    assert.deepEqual(values, [2, expected[1]])
  })

  test(`covered positional watches follow ${name}`, t => {
    const f = fixture(t)
    f.watch('rows')
    const second = f.watch('rows', 1, 'a')
    const projected = f.project({ include: ['/a'] }, 'rows', 1)
    f.db.start()
    assert.deepEqual(f.calls[0].selection, [{ include: '/rows' }])
    f.start(f.calls[0], 3, arrayDocument())
    f.revision(f.calls[0], 4, patch)
    t.mock.timers.tick(250)
    assert.deepEqual(f.cache.value.value.rows.map(row => row.a), expected)
    assert.deepEqual(second.values, [2, expected[1]])
    assert.deepEqual(projected.values, [{ a: 2 }, { a: expected[1] }])
  })
}

test('an array shift leaves unchanged positional watches quiet', t => {
  const f = fixture(t)
  f.watch('rows')
  const first = f.watch('rows', 0)
  const projected = f.project({ include: ['/a'] }, 'rows', 0)
  f.db.start()
  f.start(f.calls[0], 3, arrayDocument())
  f.revision(f.calls[0], 4, [
    { op: 'add', path: '/rows/1', value: { a: 4, b: 40 } },
    { op: 'remove', path: '/rows/3' },
  ])
  t.mock.timers.tick(250)
  assert.equal(first.values.length, 1)
  assert.equal(projected.values.length, 1)
})

test('a held array keeps its positions through insertions and removals', t => {
  const f = fixture(t)
  const second = f.watch('arr', 1)
  const rows = f.project({ include: ['/*/a'] }, 'rows')
  f.db.start()
  f.start(f.calls[0], 3, {
    arr: [null, 'b', null],
    rows: [{ a: 1 }, { a: 2 }],
  })
  assert.deepEqual(second.values, ['b'])
  f.revision(f.calls[0], 4, [
    { op: 'replace', path: '/arr', value: [null, null, 'b', null] },
    { op: 'add', path: '/rows/0', value: { a: 0, b: 9 } },
    { op: 'remove', path: '/rows/2' },
  ])
  t.mock.timers.tick(250)
  assert.deepEqual(second.values.at(-1), null)
  assert.deepEqual(rows.values.at(-1), [{ a: 0 }, { a: 1 }])
})

test('a minimal selection drops contained selectors', () => {
  assert.deepEqual(
    minimalSelection([
      { include: '/a/b' },
      { include: '/a' },
      { include: '/a', exclude: ['/a/x'] },
      { include: '/c', depth: 3 },
      { include: '/c/d', depth: 2 },
    ]).map(normalizeSelector),
    [{ include: '/a' }, { include: '/c', depth: 3 }],
  )
})

test('stop clears the document, cursor and subscription', t => {
  const f = fixture(t)
  const info = f.watch('serverInfo')
  f.db.start()
  f.start(f.calls[0], 3, { serverInfo: {} })
  f.db.stop()
  assert.equal(f.calls[0].closed, true)
  assert.deepEqual(f.cache.value, { id: 0, value: {} })
  assert.deepEqual(f.db.snapshotCursors(), {})
  info.subscription.unsubscribe()
})

const SELECTIONS = [
  [],
  [{ include: '', depth: 0 }],
  [{ include: '', depth: 1 }],
  [{ include: '', depth: 2 }],
  [{ include: '/a' }],
  [{ include: '/*/b' }],
  [{ include: '/a/*/c' }, { include: '/b' }],
  [{ include: '', exclude: ['/a/b'] }],
  [{ include: '', exclude: ['/*/c'] }],
  [{ include: '/a', exclude: ['/a/*/b'] }],
  [{ include: '/*/a', exclude: ['/b'], depth: 2 }],
  [{ include: '/arr/*/a' }],
  [{ include: '', exclude: ['/arr/*/b'] }],
  [{ include: '', exclude: [''] }],
  [{ include: '/a', exclude: ['/a/*/b'] }, { include: '/a/x/b' }],
  [{ include: '/*/b' }, { include: '/arr', exclude: ['/arr/*/b'], depth: 3 }],
  [{ include: '/arr/1' }],
  [{ include: '', exclude: ['/arr/0'] }],
  [{ include: '/arr/1/a' }, { include: '/a' }],
]

const STATES = [
  null,
  {},
  { a: { x: { b: 1, c: 2 } } },
  { a: { x: { b: 2 }, y: { c: [1] } }, b: { a: 1 } },
  { a: 5, b: { a: { c: 1 } } },
  { arr: [{ a: 1, b: 2 }] },
  { arr: [{ a: 0, b: 0 }, { a: 1, b: 2 }, { a: 3 }] },
  { arr: [{ a: 3 }] },
]

const patchesBetween = (from, to) => [
  [{ op: 'replace', path: '', value: to }],
  ...(Array.isArray(from?.arr) && Array.isArray(to?.arr)
    ? [
        [
          { op: 'add', path: '/arr/0', value: to.arr[0] ?? null },
          { op: 'remove', path: '/arr/1' },
        ],
        [{ op: 'add', path: '/arr/-', value: { a: 9 } }],
      ]
    : []),
]

test('projected operations track the projected document, as the server projects them', () => {
  for (const selection of SELECTIONS) {
    for (const from of STATES) {
      for (const to of STATES) {
        for (const patch of patchesBetween(from, to)) {
          const truth = { id: 0, value: structuredClone(from) }
          patch.forEach(op => applyOperation(truth, op))
          const projected = { id: 0, value: projectRoot(selection, from) }
          projectScoped(patch, '', selection, truth.value).forEach(op =>
            applyOperation(projected, op),
          )
          assert.deepEqual(
            projected.value,
            projectRoot(selection, truth.value),
            JSON.stringify({ selection, from, patch }),
          )
        }
      }
    }
  }
})

test('a replica applies array insertions and removals with placeholders, as it holds them', () => {
  const selection = [{ include: '/rows/*/a' }]
  const from = {
    rows: [
      { a: 1, b: 1 },
      { a: 2, b: 2 },
    ],
  }
  const patch = [
    { op: 'add', path: '/rows/0', value: { a: 0, b: 0 } },
    { op: 'remove', path: '/rows/2' },
  ]
  const truth = { id: 0, value: structuredClone(from) }
  patch.forEach(op => applyOperation(truth, op))
  const replica = { id: 0, value: projectRoot(selection, from) }
  projectOperations(patch, selection, path =>
    Array.isArray(path.reduce((v, k) => v?.[k], replica.value)),
  ).forEach(op => applyOperation(replica, op))
  assert.deepEqual(replica.value, projectRoot(selection, truth.value))
})

test('extensions add exactly what a wider selection keeps, as the server extends', () => {
  for (const held of SELECTIONS) {
    for (const wider of SELECTIONS) {
      for (const value of STATES) {
        const replica = {
          id: 0,
          value: projectRoot(wider, projectRoot(held, value)),
        }
        extension(held, wider, value).forEach(op => applyOperation(replica, op))
        assert.deepEqual(
          replica.value,
          projectRoot(wider, value),
          JSON.stringify({ held, wider, value }),
        )
      }
    }
  }
})

test('narrowing a replica projects as a whole re-projection would, sharing what it keeps', () => {
  for (const held of SELECTIONS.filter(s => s.length)) {
    for (const selection of SELECTIONS) {
      const dropped = held.filter(s => !selectionContains(selection, s))
      for (const value of STATES) {
        const replica = projectRoot(held, value)
        const narrowed = projectNarrowed(selection, dropped, replica)
        assert.deepEqual(
          narrowed,
          projectRoot(selection, replica),
          JSON.stringify({ held, selection, value }),
        )
        if (!dropped.length) assert.equal(narrowed, replica)
      }
    }
  }
  const replica = { a: { x: 1 }, b: { y: 2 } }
  const narrowed = projectNarrowed(
    [{ include: '/a' }],
    [{ include: '/b' }],
    replica,
  )
  assert.deepEqual(narrowed, { a: { x: 1 } })
  assert.equal(narrowed.a, replica.a)
})

test('reprojection matches a full projection and keeps what the change leaves', () => {
  for (const selection of SELECTIONS) {
    for (const from of STATES) {
      const previous = { source: from, value: projectRoot(selection, from) }
      assert.equal(
        reprojectRoot(selection, from, previous),
        previous.value,
        JSON.stringify({ selection, from }),
      )
      for (const to of STATES) {
        for (const patch of patchesBetween(from, to)) {
          const next = { id: 0, value: from }
          patch.forEach(op => applyOperation(next, op))
          assert.deepEqual(
            reprojectRoot(selection, next.value, previous),
            projectRoot(selection, next.value),
            JSON.stringify({ selection, from, patch }),
          )
        }
      }
    }
  }
  const selection = [{ include: '/*/title' }]
  const from = { a: { title: 'A', notes: 'n' }, b: { title: 'B', notes: 'm' } }
  const previous = { source: from, value: projectRoot(selection, from) }
  const notes = { id: 0, value: from }
  applyOperation(notes, { op: 'replace', path: '/a/notes', value: 'x' })
  assert.equal(reprojectRoot(selection, notes.value, previous), previous.value)
  const title = { id: 0, value: from }
  applyOperation(title, { op: 'replace', path: '/a/title', value: 'Z' })
  const next = reprojectRoot(selection, title.value, previous)
  assert.deepEqual(next, { a: { title: 'Z' }, b: { title: 'B' } })
  assert.equal(next.b, previous.value.b)
})

test('reconciling keeps every subtree the next document deeply equals', () => {
  for (const value of STATES) {
    assert.equal(reconcile(value, structuredClone(value)), value)
  }
  const previous = { a: { x: [1, { y: 2 }] }, b: { z: 3 }, c: 1 }
  const next = structuredClone(previous)
  next.b.z = 4
  next.d = { e: 5 }
  const merged = reconcile(previous, next)
  assert.deepEqual(merged, next)
  assert.equal(merged.a, previous.a)
  assert.notEqual(merged.b, previous.b)
  assert.deepEqual(reconcile({ a: 1, b: 2 }, { a: 1 }), { a: 1 })
  assert.deepEqual(reconcile([1, 2, 3], [1, 2]), [1, 2])
})

test('a revision notifies a projected watch only when its projection changes', t => {
  const f = fixture(t)
  const listing = f.project({ include: ['/*/title'] }, 'packages')
  f.db.start()
  f.start(f.calls[0], 3, {
    packages: { a: { title: 'A', notes: 'n' }, b: { title: 'B', notes: 'm' } },
  })
  assert.equal(listing.values.length, 1)
  f.revision(f.calls[0], 4, [
    { op: 'replace', path: '/packages/a/notes', value: 'x' },
  ])
  t.mock.timers.tick(250)
  assert.equal(listing.values.length, 1)
  f.revision(f.calls[0], 5, [
    { op: 'replace', path: '/packages/a/title', value: 'Z' },
  ])
  t.mock.timers.tick(250)
  assert.deepEqual(listing.values.at(-1), {
    a: { title: 'Z' },
    b: { title: 'B' },
  })
  assert.equal(listing.values.at(-1).b, listing.values[0].b)
})

test('a resumed subscription dump notifies no watch whose data it repeats', t => {
  const f = fixture(t)
  const info = f.watch('serverInfo')
  const ui = f.watch('ui')
  f.db.start()
  const value = { serverInfo: { name: 'box' }, ui: { theme: 1 } }
  f.start(f.calls[0], 3, structuredClone(value))
  const before = f.cache.value.value
  f.calls[0].events.error(new Error('dropped'))
  f.db.start()
  f.start(f.calls[1], 4, { ...structuredClone(value), ui: { theme: 2 } })
  assert.equal(f.cache.value.value.serverInfo, before.serverInfo)
  assert.equal(info.values.length, 1)
  assert.deepEqual(ui.values, [{ theme: 1 }, { theme: 2 }])
})

test('revisions during a selection update notify only the watches they change', t => {
  const f = fixture(t)
  const info = f.watch('serverInfo')
  const ui = f.watch('ui')
  f.db.start()
  f.start(f.calls[0], 3, { serverInfo: { name: 'box' }, ui: { theme: 1 } })
  f.watch('packageData')
  f.revision(f.calls[0], 4, [{ op: 'replace', path: '/ui/theme', value: 2 }])
  t.mock.timers.tick(250)
  assert.equal(info.values.length, 1)
  assert.deepEqual(ui.values, [{ theme: 1 }, { theme: 2 }])
  f.extend(f.calls[0], 4, [{ op: 'add', path: '/packageData', value: {} }])
  assert.equal(info.values.length, 1)
})
