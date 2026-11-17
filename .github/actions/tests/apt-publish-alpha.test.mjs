import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO, fixture } from '../../../scripts/tests/fixtures.mjs'

const publisher = yaml.load(
  readFileSync(join(REPO, '.github/workflows/apt-publish-alpha.yml'), 'utf8'),
)
const collect = publisher.jobs.publish.steps.find(
  step => step.name === 'Collect the latest master debs from every product',
).run
const REPOSITORY = 'Start9Labs/start-technologies'
const ARCHES = ['x86_64', 'aarch64', 'riscv64']

// An ISO timestamp the given number of days before now.
const daysAgo = days =>
  new Date(Date.now() - days * 86400e3).toISOString().replace(/\.\d+Z$/, 'Z')
const debs = (product, arches = ARCHES) =>
  arches.map(arch => `${product}_${arch}.deb`)
const run = (id, created, artifacts, overrides = {}) => ({
  id,
  created_at: created,
  event: 'push',
  head_branch: 'master',
  status: 'completed',
  head_sha: String(id).padStart(40, '0'),
  head_repository: { full_name: REPOSITORY },
  artifacts,
  ...overrides,
})

function collectFrom(t, runs) {
  const f = fixture(t)
  f.write('runs.json', JSON.stringify(runs))
  f.write('downloads.log', '')
  f.nodeExecutable(
    'gh',
    `import { appendFileSync, readFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
const runs = JSON.parse(readFileSync(${JSON.stringify(join(f.root, 'runs.json'))}, 'utf8'))
const args = process.argv.slice(2)
const jq = (value, filter) => {
  if (filter === null) {
    process.stdout.write(JSON.stringify(value))
    process.exit(0)
  }
  const result = spawnSync('jq', ['-r', filter], { input: JSON.stringify(value), encoding: 'utf8' })
  process.stdout.write(result.stdout)
  process.exit(result.status)
}
if (args[0] === 'api') {
  const url = args[1]
  const filter = args.includes('--jq') ? args[args.indexOf('--jq') + 1] : null
  const page = Number(new URL('https://x/' + url).searchParams.get('page') ?? 1)
  if (url.includes('/workflows/build-products.yaml/runs')) {
    const newest = [...runs].sort((a, b) => b.created_at.localeCompare(a.created_at))
    jq({ workflow_runs: newest.slice((page - 1) * 100, page * 100).map(({ artifacts, ...run }) => run) }, filter)
  }
  const artifacts = url.match(/runs\\/(\\d+)\\/artifacts/)
  if (artifacts) {
    const found = runs.find(run => String(run.id) === artifacts[1])
    jq({ artifacts: found.artifacts.map(name => ({ name, expired: name.endsWith('.expired') })) }, filter)
  }
  const single = url.match(/runs\\/(\\d+)$/)
  if (single) jq(runs.find(run => String(run.id) === single[1]), filter)
  process.exit(2)
}
if (args[0] === 'run' && args[1] === 'download') {
  appendFileSync(${JSON.stringify(join(f.root, 'downloads.log'))}, args[4] + ' ' + args[args.indexOf('-p') + 1] + '\\n')
  process.exit(0)
}
process.exit(2)
`,
  )
  const result = f.run('bash', ['-e', '-c', collect], {
    env: {
      ...f.env,
      PATH: `${f.root}:${f.env.PATH}`,
      GITHUB_REPOSITORY: REPOSITORY,
    },
    encoding: 'utf8',
  })
  const downloads = readFileSync(join(f.root, 'downloads.log'), 'utf8')
    .trim()
    .split('\n')
    .filter(Boolean)
  return { result, downloads }
}

test('each product comes from its newest master build holding every arch', t => {
  const { result, downloads } = collectFrom(t, [
    run(1, daysAgo(3), [
      ...debs('start-cli'),
      ...debs('start-tunnel'),
      ...debs('start-registry'),
    ]),
    // Packaging start-tunnel failed on riscv64.
    run(2, daysAgo(2), [
      ...debs('start-cli'),
      ...debs('start-tunnel', ['x86_64', 'aarch64']),
    ]),
    // Newer, but not a master push of this repository, or not finished.
    run(3, daysAgo(1), debs('start-cli'), {
      event: 'pull_request',
    }),
    run(4, daysAgo(1), debs('start-cli'), {
      head_repository: { full_name: 'someone/start-technologies' },
    }),
    run(5, daysAgo(1), debs('start-cli'), {
      status: 'in_progress',
    }),
    run(6, daysAgo(1), debs('start-cli'), {
      event: 'workflow_dispatch',
    }),
    run(7, daysAgo(1), debs('start-cli'), {
      head_branch: 'integration/x',
    }),
  ])
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.deepEqual(downloads.sort(), [
    '1 start-registry_*.deb',
    '1 start-tunnel_*.deb',
    '2 start-cli_*.deb',
  ])
})

test('master builds beyond pages containing only PR runs are collected', t => {
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
      'start-tunnel_x86_64.deb.expired',
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

test('runs past artifact retention end the search', t => {
  const { result, downloads } = collectFrom(t, [
    run(1, daysAgo(20), [
      ...debs('start-cli'),
      ...debs('start-tunnel'),
      ...debs('start-registry'),
    ]),
  ])
  assert.notEqual(result.status, 0)
  assert.deepEqual(downloads, [])
})
