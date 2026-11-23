import assert from 'node:assert/strict'
import test from 'node:test'
import { BehaviorSubject, Observable, Subject } from 'rxjs'
import {
  applyOperation,
  projectOperations,
  projectRoot,
  scopeOperations,
  SubtreePatchDB,
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

function fixture(t, { releaseDelay = 5000 } = {}) {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const calls = []
  const source = {
    subscribe(path, since, projection) {
      const events = new Subject()
      const call = {
        path: path.length ? '/' + path.join('/') : '',
        since,
        projection,
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
  }
  const cache = new BehaviorSubject({ id: 0, value: {} })
  const db = new SubtreePatchDB(source, cache, { releaseDelay, batch: 250 })
  const open = path => calls.filter(c => c.path === path && !c.closed)
  const start = (call, id, value, epoch = 'boot') =>
    call.events.next({ epoch, id, value })
  const revision = (call, id, patch) => call.events.next({ id, patch })
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
  return { db, cache, calls, open, start, revision, watch, project }
}

test('a watched path opens one server subscription shared by its watchers and released after a delay', t => {
  const f = fixture(t)
  f.db.start()
  const a = f.watch('serverInfo')
  const b = f.watch('serverInfo')
  assert.equal(f.calls.length, 1)
  assert.equal(f.calls[0].path, '/serverInfo')
  assert.equal(f.calls[0].since, null)
  f.start(f.calls[0], 5, { hostname: 'box' })
  assert.deepEqual(a.values, [{ hostname: 'box' }])
  assert.deepEqual(b.values, [{ hostname: 'box' }])
  assert.equal(f.cache.value.id, 5)

  a.subscription.unsubscribe()
  b.subscription.unsubscribe()
  t.mock.timers.tick(4999)
  assert.equal(f.calls[0].closed, false)
  const again = f.watch('serverInfo')
  t.mock.timers.tick(10_000)
  assert.equal(f.calls[0].closed, false)
  assert.deepEqual(again.values, [{ hostname: 'box' }])
  again.subscription.unsubscribe()
  t.mock.timers.tick(5000)
  assert.equal(f.calls[0].closed, true)
  assert.equal(f.calls.length, 1)
})

test('an ancestor subscription serves its descendants and replaces their subscriptions once live', t => {
  const f = fixture(t)
  f.db.start()
  const child = f.watch('packageData', 'a')
  assert.deepEqual(
    f.calls.map(c => c.path),
    ['/packageData/a'],
  )
  f.start(f.calls[0], 3, { v: 1 })
  assert.deepEqual(child.values, [{ v: 1 }])

  const parent = f.watch('packageData')
  assert.equal(f.calls[1].path, '/packageData')
  assert.equal(f.calls[1].since, null)
  assert.equal(f.calls[0].closed, false)
  f.start(f.calls[1], 4, { a: { v: 2 }, b: { v: 1 } })
  assert.equal(f.calls[0].closed, true)
  assert.equal(f.cache.value.id, 4)
  assert.deepEqual(child.values.at(-1), { v: 2 })
  assert.deepEqual(parent.values.at(-1), { a: { v: 2 }, b: { v: 1 } })
  f.watch('packageData', 'b')
  assert.equal(f.calls.length, 2)

  parent.subscription.unsubscribe()
  t.mock.timers.tick(5000)
  assert.equal(f.calls[1].closed, true)
  const reopened = f.open('/packageData/a')
  assert.equal(reopened.length, 1)
  assert.deepEqual(reopened[0].since, { epoch: 'boot', id: 4 })
})

test('revisions from different subscriptions apply in id order per batch and advance the document id', t => {
  const f = fixture(t)
  f.db.start()
  const info = f.watch('serverInfo')
  const ui = f.watch('ui')
  const [serverInfo, uiCall] = f.calls
  f.start(serverInfo, 10, { n: 0 })
  f.start(uiCall, 10, { n: 0 })
  f.revision(uiCall, 12, [{ op: 'replace', path: '/n', value: 12 }])
  f.revision(serverInfo, 11, [{ op: 'replace', path: '/n', value: 11 }])
  f.revision(serverInfo, 11, [{ op: 'replace', path: '/n', value: 99 }])
  assert.equal(f.cache.value.id, 10)
  t.mock.timers.tick(250)
  assert.deepEqual(info.values.at(-1), { n: 11 })
  assert.deepEqual(ui.values.at(-1), { n: 12 })
  assert.equal(f.cache.value.id, 11)
  f.revision(serverInfo, 12, [])
  t.mock.timers.tick(250)
  assert.equal(f.cache.value.id, 12)
})

test('a restored snapshot serves watchers before going live and each path resumes from its cursor', t => {
  const f = fixture(t)
  f.db.restore(
    { id: 7, value: { serverInfo: { hostname: 'cached' }, ui: { a: 1 } } },
    {
      '/serverInfo': { epoch: 'boot', id: 7 },
      '/ui': { epoch: 'boot', id: 6 },
    },
  )
  const info = f.watch('serverInfo', 'hostname')
  const registry = f.watch('registryCache')
  assert.deepEqual(info.values, ['cached'])
  assert.deepEqual(registry.values, [])
  f.watch('ui')
  f.db.start()
  const since = Object.fromEntries(f.calls.map(c => [c.path, c.since]))
  assert.deepEqual(since, {
    '/serverInfo/hostname': { epoch: 'boot', id: 7 },
    '/registryCache': null,
    '/ui': { epoch: 'boot', id: 6 },
  })
  f.start(f.open('/registryCache')[0], 8, { url: {} })
  assert.deepEqual(registry.values, [{ url: {} }])
})

test('a failed subscription stops them all, keeps data and cursors, and start resumes every path', t => {
  const f = fixture(t)
  const failures = []
  f.db.failures$.subscribe(e => failures.push(e))
  f.db.start()
  const info = f.watch('serverInfo')
  f.watch('ui')
  const live = []
  f.db.live$(['serverInfo'], ['ui']).subscribe(v => live.push(v))
  f.start(f.calls[0], 5, { hostname: 'box' })
  assert.deepEqual(live, [false])
  f.start(f.calls[1], 5, {})
  assert.deepEqual(live, [false, true])
  f.revision(f.calls[0], 6, [])
  f.calls[1].events.error(new Error('lost'))
  assert.equal(failures.length, 1)
  assert.deepEqual(live, [false, true, false])
  assert.equal(f.calls[0].closed, true)
  assert.deepEqual(info.values, [{ hostname: 'box' }])
  f.db.start()
  assert.deepEqual(
    f.calls.slice(2).map(c => [c.path, c.since]),
    [
      ['/serverInfo', { epoch: 'boot', id: 6 }],
      ['/ui', { epoch: 'boot', id: 5 }],
    ],
  )
})

test('a deep subscription creates the parents its dump needs', t => {
  const f = fixture(t)
  f.db.start()
  const deep = f.watch('packageData', 'x', 'status')
  f.start(f.calls[0], 2, 'running')
  assert.deepEqual(deep.values, ['running'])
  assert.deepEqual(f.cache.value.value, {
    packageData: { x: { status: 'running' } },
  })
  f.revision(f.calls[0], 3, [{ op: 'replace', path: '', value: 'stopped' }])
  t.mock.timers.tick(250)
  assert.deepEqual(deep.values.at(-1), 'stopped')
})

for (const mode of ['replay', 'revision']) {
  test(`a child ${mode} materializes its cached null parent`, t => {
    const f = fixture(t)
    f.db.start()
    const parent = f.watch('a')
    f.start(f.calls[0], 5, null)
    const child = f.watch('a', 'b')
    parent.subscription.unsubscribe()
    t.mock.timers.tick(5000)
    const call = f.open('/a/b')[0]
    assert.deepEqual(call.since, { epoch: 'boot', id: 5 })
    const revision = {
      id: 6,
      patch: [{ op: 'replace', path: '', value: 1 }],
    }
    call.events.next({
      epoch: 'boot',
      id: mode === 'replay' ? 6 : 5,
      revisions: mode === 'replay' ? [revision] : [],
    })
    if (mode === 'revision') {
      call.events.next(revision)
      t.mock.timers.tick(250)
    }
    assert.equal(child.values.at(-1), 1)
    assert.deepEqual(f.cache.value.value, { a: { b: 1 } })
  })
}

test('stop discards released paths rather than reopening them on start', t => {
  const f = fixture(t)
  f.db.start()
  const root = f.watch()
  f.start(f.calls[0], 5, { serverInfo: {} })
  root.subscription.unsubscribe()
  f.db.stop()
  f.db.start()
  assert.equal(f.calls.length, 1)
  assert.equal(f.calls[0].closed, true)
})

test('stop clears the document, cursors and subscriptions', t => {
  const f = fixture(t)
  f.db.start()
  f.watch('serverInfo')
  f.start(f.calls[0], 5, { hostname: 'box' })
  f.db.stop()
  assert.equal(f.calls[0].closed, true)
  assert.deepEqual(f.cache.value, { id: 0, value: {} })
  assert.deepEqual(f.db.snapshotCursors(), {})
})

const PROJECTIONS = [
  null,
  { depth: 0 },
  { depth: 1 },
  { depth: 2 },
  { include: ['/a'] },
  { include: ['/*/b'] },
  { include: ['/a/*/c', '/b'] },
  { exclude: ['/a/b'] },
  { exclude: ['/*/c'] },
  { include: ['/a'], exclude: ['/a/*/b'] },
  { depth: 2, include: ['/*/a'], exclude: ['/b'] },
  { include: ['/arr/*/a'] },
  { exclude: ['/arr/*/b'] },
  { exclude: [''] },
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

function replaceAll(to) {
  return [{ op: 'replace', path: '', value: to }]
}

test('projected operations track the projected document, as the server projects them', () => {
  const patches = (from, to) => [
    replaceAll(to),
    ...(from?.arr && to?.arr
      ? [
          [
            { op: 'add', path: '/arr/0', value: to.arr[0] ?? null },
            { op: 'remove', path: '/arr/1' },
          ],
        ]
      : []),
  ]
  for (const projection of PROJECTIONS) {
    for (const from of STATES) {
      for (const to of STATES) {
        for (const patch of patches(from, to)) {
          const truth = { id: 0, value: structuredClone(from) }
          patch.forEach(op => applyOperation(truth, op))
          const projected = { id: 0, value: projectRoot(projection, from) }
          projectOperations(scopeOperations(patch, ''), projection).forEach(
            op => applyOperation(projected, op),
          )
          assert.deepEqual(
            projected.value,
            projectRoot(projection, truth.value),
            JSON.stringify({ projection, from, patch }),
          )
        }
      }
    }
  }
})

test('a projected subscription opens with its projection and writes only what it includes', t => {
  const f = fixture(t)
  f.db.start()
  const full = f.watch('packageData', 'x')
  f.start(f.calls[0], 3, { state: 1, icon: 'big' })
  const listing = f.project({ exclude: ['/*/icon'] }, 'packageData')
  const call = f.calls[1]
  assert.equal(call.path, '/packageData')
  assert.deepEqual(call.projection, { exclude: ['/*/icon'] })
  assert.equal(call.since, null)
  f.start(call, 4, { x: { state: 2 }, y: { state: 1 } })
  assert.deepEqual(listing.values.at(-1), {
    x: { state: 2 },
    y: { state: 1 },
  })
  assert.deepEqual(full.values.at(-1), { state: 2, icon: 'big' })
  assert.equal(f.calls[0].closed, false)

  f.revision(call, 5, [{ op: 'replace', path: '/x', value: { state: 3 } }])
  f.revision(call, 6, [{ op: 'remove', path: '/y' }])
  f.revision(f.calls[0], 5, [{ op: 'replace', path: '/state', value: 3 }])
  t.mock.timers.tick(250)
  assert.deepEqual(f.cache.value.value.packageData, {
    x: { state: 3, icon: 'big' },
  })
  assert.deepEqual(listing.values.at(-1), { x: { state: 3 } })
  assert.equal(f.cache.value.id, 5)
})

test('an unprojected ancestor serves a projected watch', t => {
  const f = fixture(t)
  f.db.start()
  f.watch('packageData')
  f.start(f.calls[0], 3, { x: { state: 1, icon: 'big' } })
  const projected = f.project({ exclude: ['/*/icon'] }, 'packageData')
  assert.equal(f.calls.length, 1)
  assert.deepEqual(projected.values, [{ x: { state: 1 } }])
})

test('a projection serves only watches of the same projection', t => {
  const f = fixture(t)
  f.db.start()
  f.project({ include: ['/*/state'] }, 'packageData')
  f.start(f.calls[0], 3, { x: { state: 1 } })
  const child = f.watch('packageData', 'x')
  assert.deepEqual(
    f.calls.map(c => [c.path, c.projection]),
    [
      ['/packageData', { include: ['/*/state'] }],
      ['/packageData/x', null],
    ],
  )
  assert.equal(f.calls[1].since, null)
  assert.deepEqual(child.values, [])
  f.start(f.calls[1], 4, { state: 2, icon: 'big' })
  assert.deepEqual(child.values, [{ state: 2, icon: 'big' }])
})

test('a projected subscription resumes from its own cursor only without overlapping data', t => {
  const f = fixture(t)
  f.db.start()
  const projection = { include: ['/*/state'] }
  const listing = f.project(projection, 'packageData')
  f.start(f.calls[0], 3, { x: { state: 1 } })
  assert.deepEqual(f.db.snapshotCursors(), {
    '["/packageData",{"include":["/*/state"]}]': { epoch: 'boot', id: 3 },
  })
  listing.subscription.unsubscribe()
  t.mock.timers.tick(5000)
  f.project(projection, 'packageData')
  assert.deepEqual(f.calls[1].since, { epoch: 'boot', id: 3 })

  const child = f.watch('packageData', 'x')
  f.start(f.calls[2], 4, { state: 2, icon: 'big' })
  child.subscription.unsubscribe()
  f.calls[1].events.next({ epoch: 'boot', id: 4, revisions: [] })
  t.mock.timers.tick(5000)
  f.db.stop()
  f.db.restore(
    { id: 4, value: { packageData: { x: { state: 2, icon: 'big' } } } },
    {
      '["/packageData",{"include":["/*/state"]}]': { epoch: 'boot', id: 4 },
      '/packageData/x': { epoch: 'boot', id: 4 },
    },
  )
  f.db.start()
  f.project(projection, 'packageData')
  assert.equal(f.calls.at(-1).since, null)
})
