import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import yaml from 'js-yaml'
import { fixture } from '../../../../scripts/tests/fixtures.mjs'

const workflow = yaml.load(
  readFileSync(
    new URL('../../../../.github/workflows/nut-updates.yml', import.meta.url),
    'utf8',
  ),
)
const script = fileURLToPath(new URL('./open-update-pr.sh', import.meta.url))
const pins = 'projects/start-os/build/nut/version.env'
const fragment = 'projects/start-os/changelog/patch-changed-nut-update.md'
const candidate = '2.8.6-1'
const branch = `automation/nut-${createHash('sha256').update(candidate).digest('hex').slice(0, 16)}`

test('scheduled automation confines write permission to publishing after all architectures pass', () => {
  assert.deepEqual(Object.keys(workflow.on).sort(), [
    'schedule',
    'workflow_dispatch',
  ])
  assert.equal(workflow.jobs.prepare.if, "github.ref == 'refs/heads/master'")
  assert.deepEqual(workflow.permissions, {
    contents: 'read',
    'pull-requests': 'read',
  })
  assert.deepEqual(workflow.jobs.propose.needs, ['prepare', 'verify'])
  assert.equal(
    workflow.jobs.propose.if,
    "needs.prepare.outputs.changed == 'true'",
  )
  assert.deepEqual(workflow.jobs.propose.permissions, {
    contents: 'write',
    'pull-requests': 'write',
  })
  assert.deepEqual(
    workflow.jobs.verify.strategy.matrix.include.map(row => row.arch).sort(),
    ['aarch64', 'riscv64', 'x86_64'],
  )
  for (const job of [workflow.jobs.prepare, workflow.jobs.verify])
    assert.equal(job.permissions, undefined)
  for (const job of Object.values(workflow.jobs)) {
    const checkout = job.steps.find(step => step.uses === 'actions/checkout@v6')
    assert.equal(checkout.with['persist-credentials'], false)
  }
  const verification = workflow.jobs.verify.steps.find(
    step => step.name === 'Build and test fresh and upgrade installations',
  )
  assert.equal(verification.run, 'make start-os-nut start-os-nut-test')
  assert.equal(workflow.concurrency['cancel-in-progress'], false)
})

function proposal(t) {
  const f = fixture(t)
  f.git('init', '-b', 'master')
  f.git('config', 'user.name', 'Fixture')
  f.git('config', 'user.email', 'fixture@example.invalid')
  f.git('config', 'commit.gpgsign', 'false')
  f.write(pins, 'NUT_VERSION=2.8.5\n')
  f.write(fragment, 'old fragment\n')
  f.write(
    'scripts/commit-staged.mjs',
    readFileSync(
      new URL('../../../../scripts/commit-staged.mjs', import.meta.url),
    ),
  )
  const renderer = 'projects/start-os/build/nut/render-update-pr.mjs'
  f.write(
    renderer,
    readFileSync(new URL('./render-update-pr.mjs', import.meta.url)),
  )
  f.git('add', pins, fragment, 'scripts/commit-staged.mjs', renderer)
  f.git('commit', '-m', 'fixture')
  const base = f.git('rev-parse', 'HEAD').trim()
  f.git('init', '--bare', 'remote.git')
  f.git(
    '--git-dir',
    join(f.root, 'remote.git'),
    'fetch',
    f.root,
    'master:refs/heads/master',
  )
  f.git('remote', 'add', 'origin', join(f.root, 'remote.git'))
  f.write(pins, 'NUT_VERSION=2.8.6\n')
  f.write(fragment, 'new fragment\n')
  f.write(
    'proposal/update.patch',
    f.git('diff', '--binary', 'HEAD', '--', pins, fragment),
  )
  f.git('restore', '--worktree', '--', pins, fragment)
  f.write(
    'proposal/report.json',
    JSON.stringify({
      current: '2.8.5-1',
      candidate,
      files: [pins, fragment],
      releaseUrl: 'https://example.invalid/release',
    }),
  )
  f.write('proposal/release-aligned', 'true\n')
  f.write('proposal/release-check.txt', 'version alignment required\n')
  f.nodeExecutable(
    'gh',
    `
import { appendFileSync, readFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
const root = ${JSON.stringify(f.root)}
const args = process.argv.slice(2)
appendFileSync(root + '/calls.jsonl', JSON.stringify(args) + '\\n')
const git = (a, options = {}) => execFileSync('git', ['--git-dir', root + '/remote.git', ...a], { encoding: 'utf8', ...options }).trim()
if (args[0] === 'pr' && args[1] === 'list') {
  if (process.env.PROPOSAL_MODE === 'existing') console.log('https://example.invalid/pull/1')
} else if (args[0] === 'pr' && args[1] === 'create') {
  if (process.env.PROPOSAL_MODE === 'fail-create') process.exit(1)
  console.log('https://example.invalid/pull/1')
} else if (args[0] === 'api' && args[1].endsWith('/git/refs')) {
  git(['update-ref', args.find(v => v.startsWith('ref=')).slice(4), args.find(v => v.startsWith('sha=')).slice(4)])
} else if (args[0] === 'api' && args[1] === 'graphql') {
  if (process.env.PROPOSAL_MODE === 'fail-commit') process.exit(1)
  const payload = JSON.parse(readFileSync(0, 'utf8')).variables.input
  const env = { ...process.env, GIT_INDEX_FILE: root + '/remote-index', GIT_AUTHOR_NAME: 'Bot', GIT_AUTHOR_EMAIL: 'bot@example.invalid', GIT_COMMITTER_NAME: 'Bot', GIT_COMMITTER_EMAIL: 'bot@example.invalid' }
  git(['read-tree', payload.expectedHeadOid], { env })
  for (const file of payload.fileChanges.additions) {
    const blob = git(['hash-object', '-w', '--stdin'], { input: Buffer.from(file.contents, 'base64'), env })
    git(['update-index', '--add', '--cacheinfo', '100644', blob, file.path], { env })
  }
  const tree = git(['write-tree'], { env })
  const oid = git(['commit-tree', tree, '-p', payload.expectedHeadOid], { input: payload.message.headline, env })
  git(['update-ref', 'refs/heads/' + payload.branch.branchName, oid, payload.expectedHeadOid], { env })
  console.log(JSON.stringify({ data: { createCommitOnBranch: { commit: { oid } } } }))
} else throw new Error('Unexpected gh call: ' + args)
`,
  )
  const run = mode =>
    f.run('bash', [script], {
      encoding: 'utf8',
      env: {
        ...f.env,
        PATH: `${f.root}:${f.env.PATH}`,
        PROPOSAL_MODE: mode,
        UPDATE_DIRECTORY: join(f.root, 'proposal'),
        GH_TOKEN: 'fixture-token',
        GITHUB_REPOSITORY: 'fixture/repo',
        GITHUB_SERVER_URL: 'https://example.invalid',
        GITHUB_RUN_ID: '123',
      },
    })
  const calls = () =>
    readFileSync(join(f.root, 'calls.jsonl'), 'utf8')
      .trim()
      .split('\n')
      .map(line => JSON.parse(line))
  const reset = () =>
    f.git('restore', '--staged', '--worktree', '--', pins, fragment)
  return { ...f, runProposal: run, calls, reset, base }
}

test('opens a labeled PR through the signed commit helper without writing master', t => {
  const f = proposal(t)
  const result = f.runProposal('ready')
  assert.equal(result.status, 0, result.stderr)
  const calls = f.calls()
  assert.equal(calls.filter(args => args[1] === 'graphql').length, 1)
  const create = calls.find(args => args[0] === 'pr' && args[1] === 'create')
  assert.ok(
    create.includes('StartOS') &&
      create.includes('repo') &&
      create.includes('patch'),
  )
  assert.ok(!create.includes('--draft'))
  assert.equal(
    f
      .git('--git-dir', join(f.root, 'remote.git'), 'rev-parse', 'master')
      .trim(),
    f.base,
  )
  const body = readFileSync(join(f.root, 'proposal/pr-body.md'), 'utf8')
  assert.match(body, /actions\/runs\/123/)
  assert.match(body, /2\.8\.5-1.*2\.8\.6-1/)
})

test('release alignment failures produce a draft with actionable check output', t => {
  const f = proposal(t)
  f.write('proposal/release-aligned', 'false\n')
  const result = f.runProposal('ready')
  assert.equal(result.status, 0, result.stderr)
  assert.ok(
    f
      .calls()
      .find(args => args[1] === 'create')
      .includes('--draft'),
  )
  assert.match(
    readFileSync(join(f.root, 'proposal/pr-body.md'), 'utf8'),
    /VERSION_BUMP\.md[\s\S]*version alignment required/,
  )
})

test('an existing proposal is left untouched', t => {
  const f = proposal(t)
  f.write('proposal/update.patch', 'invalid patch proves it is never applied')
  assert.equal(f.runProposal('existing').status, 0)
  assert.equal(f.calls().length, 1)
  assert.equal(f.git('diff', '--name-only'), '')
})

for (const mode of ['fail-create', 'fail-commit']) {
  test(`publication resumes after ${mode} without rewriting history`, t => {
    const f = proposal(t)
    assert.notEqual(f.runProposal(mode).status, 0)
    f.reset()
    const result = f.runProposal('ready')
    assert.equal(result.status, 0, result.stderr)
    const commits = f.calls().filter(args => args[1] === 'graphql')
    assert.equal(commits.length, mode === 'fail-create' ? 1 : 2)
    assert.ok(!f.calls().some(args => args.includes('--force')))
  })
}

test('a maintainer edit outside the verified update blocks publication', t => {
  const f = proposal(t)
  assert.notEqual(f.runProposal('fail-create').status, 0)
  f.reset()
  f.git('fetch', 'origin', branch)
  f.git('switch', '--detach', 'FETCH_HEAD')
  f.write('human-change.txt', 'unvalidated change\n')
  f.git('add', 'human-change.txt')
  f.git('commit', '-m', 'human edit')
  const human = f.git('rev-parse', 'HEAD').trim()
  f.git('--git-dir', join(f.root, 'remote.git'), 'fetch', f.root, 'HEAD')
  f.git(
    '--git-dir',
    join(f.root, 'remote.git'),
    'update-ref',
    `refs/heads/${branch}`,
    human,
  )
  f.git('switch', '--detach', f.base)
  const result = f.runProposal('ready')
  assert.notEqual(result.status, 0)
  assert.match(result.stdout, /outside the verified update/)
  assert.equal(
    f.git('--git-dir', join(f.root, 'remote.git'), 'rev-parse', branch).trim(),
    human,
  )
})
