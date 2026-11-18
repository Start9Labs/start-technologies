import assert from 'node:assert/strict'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO } from './fixtures.mjs'
import { artifactFixture } from './gha-artifact-fixture.mjs'

const GLOB = 'startos-*_x86_64.squashfs'
const image = (hash, platform = 'x86_64') =>
  `startos-0.4.0-${hash}_${platform}.squashfs`
const run = (id, head_branch, artifacts) => ({ id, head_branch, artifacts })

function discover(t, runs, env = {}) {
  const f = artifactFixture(t, runs)
  const result = f.run(
    'bash',
    [
      join(REPO, 'scripts/gha-artifact-run-ids.sh'),
      'Start9Labs/start-technologies',
      GLOB,
      'master',
    ],
    { env: { ...f.env, ...env }, encoding: 'utf8' },
  )
  return { result, calls: f.calls() }
}

test('discovery selects unexpired matching artifacts for the requested branch, newest first', t => {
  const { result } = discover(t, [
    run(1, 'master', [image('aaaaaaa')]),
    run(2, 'master', [image('bbbbbbb')]),
    run(3, 'integration/other', [image('ccccccc')]),
    run(4, 'master', [
      image('ddddddd', 'aarch64'),
      image('ddddddd', 'x86_64-nonfree'),
      'startos-0.4.0-ddddddd_x86_64.migration.squashfs',
    ]),
  ])
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.deepEqual(result.stdout.trim().split('\n'), ['2', '1'])
})

test('discovery includes matching artifacts beyond the first page', t => {
  const { result } = discover(t, [
    run(1, 'master', [image('aaaaaaa')]),
    ...Array.from({ length: 101 }, (_, index) =>
      run(index + 2, 'integration/other', ['start-cli_x86_64.deb']),
    ),
  ])
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.equal(result.stdout.trim(), '1')
})

test('expired matches do not hide unexpired artifacts on later pages', t => {
  const { result } = discover(t, [
    run(1, 'master', [image('aaaaaaa')]),
    ...Array.from({ length: 150 }, (_, index) =>
      run(index + 2, 'integration/other', ['start-cli_x86_64.deb']),
    ),
    run(200, 'integration/other', [{ name: image('bbbbbbb'), expired: true }]),
    run(201, 'master', [image('ccccccc')]),
  ])
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.deepEqual(result.stdout.trim().split('\n'), ['201', '1'])
})

test('artifact discovery propagates API failures', t => {
  const { result } = discover(t, [run(1, 'master', [image('aaaaaaa')])], {
    GH_FAIL_MATCH: '/actions/artifacts',
  })
  assert.notEqual(result.status, 0)
  assert.equal(result.stdout, '')
})
