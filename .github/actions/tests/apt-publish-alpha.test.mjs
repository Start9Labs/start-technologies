import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO } from '../../../scripts/tests/fixtures.mjs'
import { artifactFixture } from '../../../scripts/tests/gha-artifact-fixture.mjs'

const publisher = yaml.load(
  readFileSync(join(REPO, '.github/workflows/apt-publish-alpha.yml'), 'utf8'),
)
const collect = publisher.jobs.publish.steps.find(
  step => step.name === 'Collect the latest master debs from every product',
).run
const REPOSITORY = 'Start9Labs/start-technologies'
const ARCHES = ['x86_64', 'aarch64', 'riscv64']

const daysAgo = days =>
  new Date(Date.now() - days * 86400e3).toISOString().replace(/\.\d+Z$/, 'Z')
const debs = (product, arches = ARCHES) =>
  arches.map(arch => `${product}_${arch}.deb`)
const run = (id, created, artifacts, overrides = {}) => ({
  id,
  created_at: created,
  event: 'push',
  path: '.github/workflows/build-products.yaml',
  head_branch: 'master',
  status: 'completed',
  head_sha: String(id).padStart(40, '0'),
  head_repository: { full_name: REPOSITORY },
  artifacts,
  ...overrides,
})

function collectFrom(t, runs, env = {}) {
  const f = artifactFixture(t, runs)
  const result = f.run('bash', ['-e', '-c', collect], {
    env: { ...f.env, GITHUB_REPOSITORY: REPOSITORY, ...env },
    encoding: 'utf8',
  })
  return { result, downloads: f.downloads(), calls: f.calls() }
}

test('each product comes from its newest master build holding every arch', t => {
  const { result, downloads } = collectFrom(t, [
    run(1, daysAgo(3), [
      ...debs('start-cli'),
      ...debs('start-tunnel'),
      ...debs('start-registry'),
    ]),
    run(2, daysAgo(2), [
      ...debs('start-cli'),
      ...debs('start-tunnel', ['x86_64', 'aarch64']),
    ]),
    run(3, daysAgo(1), debs('start-cli'), { event: 'pull_request' }),
    run(4, daysAgo(1), debs('start-cli'), {
      head_repository: { full_name: 'someone/start-technologies' },
    }),
    run(5, daysAgo(1), debs('start-cli'), { status: 'in_progress' }),
    run(6, daysAgo(1), debs('start-cli'), { event: 'workflow_dispatch' }),
    run(7, daysAgo(1), debs('start-cli'), { head_branch: 'integration/x' }),
    run(8, daysAgo(1), debs('start-cli'), {
      path: '.github/workflows/other.yaml',
    }),
  ])
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.deepEqual(downloads.sort(), [
    '1 start-registry_*.deb',
    '1 start-tunnel_*.deb',
    '2 start-cli_*.deb',
  ])
})

test('PR-only activity does not hide complete master artifacts', t => {
  const { result, downloads } = collectFrom(t, [
    run(1, daysAgo(3), [
      ...debs('start-cli'),
      ...debs('start-tunnel'),
      ...debs('start-registry'),
    ]),
    ...Array.from({ length: 301 }, (_, index) =>
      run(index + 2, daysAgo(2), [], { event: 'pull_request' }),
    ),
  ])
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.deepEqual(downloads.sort(), [
    '1 start-cli_*.deb',
    '1 start-registry_*.deb',
    '1 start-tunnel_*.deb',
  ])
})

test('a product without a complete build keeps its published package', t => {
  const { result, downloads } = collectFrom(t, [
    run(1, daysAgo(3), [
      ...debs('start-cli'),
      ...debs('start-registry', ['x86_64']),
      { name: 'start-tunnel_x86_64.deb', expired: true },
    ]),
  ])
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.deepEqual(downloads, ['1 start-cli_*.deb'])
  assert.match(result.stdout, /start-tunnel has no complete master build left/)
  assert.match(
    result.stdout,
    /start-registry has no complete master build left/,
  )
})

test('nothing to collect fails the publish', t => {
  const { result, downloads } = collectFrom(t, [])
  assert.notEqual(result.status, 0)
  assert.deepEqual(downloads, [])
})

test('an older run with unexpired rerun artifacts remains eligible', t => {
  const { result, downloads } = collectFrom(t, [
    run(
      1,
      daysAgo(20),
      [
        ...debs('start-cli'),
        ...debs('start-tunnel'),
        ...debs('start-registry'),
      ],
      { run_attempt: 2 },
    ),
  ])
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.deepEqual(downloads.sort(), [
    '1 start-cli_*.deb',
    '1 start-registry_*.deb',
    '1 start-tunnel_*.deb',
  ])
})

test('expired artifacts do not cause run metadata queries', t => {
  const { result, calls } = collectFrom(t, [
    run(1, daysAgo(20), [{ name: 'start-cli_x86_64.deb', expired: true }]),
  ])
  assert.notEqual(result.status, 0)
  assert.equal(
    calls.some(args => args.some(arg => arg.includes('/actions/runs/'))),
    false,
  )
})

test('a failed metadata query does not publish a partial selection', t => {
  const { result, downloads } = collectFrom(
    t,
    [
      run(1, daysAgo(2), debs('start-cli')),
      run(2, daysAgo(1), debs('start-cli')),
    ],
    { GH_FAIL_MATCH: '/actions/runs/1' },
  )
  assert.notEqual(result.status, 0)
  assert.deepEqual(downloads, [])
})
