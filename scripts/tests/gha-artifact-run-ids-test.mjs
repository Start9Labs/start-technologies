import assert from 'node:assert/strict'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO } from './fixtures.mjs'
import { artifactFixture } from './gha-artifact-fixture.mjs'

const IMAGE = 'x86_64.squashfs'
const run = (id, head_branch, artifacts) => ({ id, head_branch, artifacts })

function discover(t, runs, env = {}) {
  const f = artifactFixture(t, runs)
  return f.run(
    'bash',
    [
      join(REPO, 'scripts/gha-artifact-run-ids.sh'),
      'Start9Labs/start-technologies',
      IMAGE,
      'master',
    ],
    { env: { ...f.env, ...env }, encoding: 'utf8' },
  )
}

test('discovery selects unexpired artifacts for the requested branch', t => {
  const result = discover(t, [
    run(1, 'master', [IMAGE]),
    run(2, 'master', [{ name: IMAGE, expired: true }]),
    run(3, 'integration/other', [IMAGE]),
    run(4, 'master', ['aarch64.squashfs']),
  ])
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.equal(result.stdout.trim(), '1')
})

test('discovery includes matching artifacts beyond the first page', t => {
  const result = discover(t, [
    ...Array.from({ length: 101 }, (_, index) =>
      run(index + 1, 'integration/other', [IMAGE]),
    ),
    run(200, 'master', [IMAGE]),
  ])
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.equal(result.stdout.trim(), '200')
})

test('artifact discovery propagates API failures', t => {
  const result = discover(t, [run(1, 'master', [IMAGE])], {
    GH_FAIL_MATCH: '/actions/artifacts',
  })
  assert.notEqual(result.status, 0)
  assert.equal(result.stdout, '')
})
