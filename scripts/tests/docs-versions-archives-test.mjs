import assert from 'node:assert/strict'
import test from 'node:test'
import { reconcile } from '../docs-versions.mjs'

const PUBLISHED = `start-os=0.4.1.x
start-os=0.4.0.x@start-os/v0.4.0.4
start-tunnel=1.5.x
start-wrt=1.3.x
packaging
`

const TAGGED = `start-os=0.4.2.x
start-os=0.4.1.x@start-os/v0.4.1.2
start-os=0.4.0.x@start-os/v0.4.0.4
start-tunnel=1.6.x
start-wrt=1.4.x
packaging
`

test('a StartOS tag labels its current book without replacing archives', () => {
  assert.equal(
    reconcile(TAGGED, PUBLISHED, 'start-os', '0.4.2'),
    `start-os=0.4.2.x
start-os=0.4.1.x@start-os/v0.4.1.2
start-os=0.4.0.x@start-os/v0.4.0.4
start-tunnel=1.5.x
start-wrt=1.3.x
packaging
`,
  )
})

test('another product tag keeps the current book out of archives', () => {
  assert.equal(
    reconcile(TAGGED, PUBLISHED, 'start-wrt', '1.4.0'),
    `start-os=0.4.1.x
start-os=0.4.0.x@start-os/v0.4.0.4
start-tunnel=1.5.x
start-wrt=1.4.x
packaging
`,
  )
})

test('an older tag cannot replace archives in a retained site tree', () => {
  assert.equal(
    reconcile(TAGGED, PUBLISHED, 'start-wrt', '1.4.0', PUBLISHED),
    `start-os=0.4.1.x
start-os=0.4.0.x@start-os/v0.4.0.4
start-tunnel=1.5.x
start-wrt=1.4.x
packaging
`,
  )
})

const NEXT = `start-os=0.4.3.x
start-os=0.4.2.x@start-os/v0.4.2
start-os=0.4.1.x@start-os/v0.4.1.2
start-os=0.4.0.x@start-os/v0.4.0.4
start-tunnel=1.6.x
start-wrt=1.4.x
packaging
`

test('the next StartOS minor archives the shipped 0.4.2 book', () => {
  assert.equal(reconcile(NEXT, TAGGED, 'start-os', '0.4.3'), NEXT)
})

test('another product release does not publish the pending 0.4.3 book', () => {
  assert.equal(reconcile(TAGGED, TAGGED, 'start-wrt', '1.4.1', NEXT), TAGGED)
})

test('a hand-labelled book uses its current entry', () => {
  const tunnel = `start-tunnel=1.6.x
start-tunnel=1.5.x@start-tunnel/v1.5.0
`
  assert.equal(reconcile(tunnel, tunnel, 'start-tunnel', '1.6.0'), tunnel)
})
