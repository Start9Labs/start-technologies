import assert from 'node:assert/strict'
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
} from 'node:fs'
import { join } from 'node:path'
import test from 'node:test'
import { fixture, REPO } from './fixtures.mjs'

const SCRIPT = join(REPO, 'scripts/commit-staged.mjs')

function commitFixture(t) {
  const f = fixture(t)
  f.git('init', '-q')
  f.git('config', 'user.name', 'Fixture')
  f.git('config', 'user.email', 'fixture@example.com')
  f.git('config', 'commit.gpgsign', 'false')
  f.write('old.md', 'old\n')
  f.git('add', '.')
  f.git('commit', '-qm', 'fixture')
  f.base = f.git('rev-parse', 'HEAD').trim()
  f.capture = join(f.root, 'requests.jsonl')
  f.message = f.write('message', 'chore: archive fragments\n\nDetails.\n')
  f.nodeExecutable(
    'bin/gh',
    `import { createHash } from 'node:crypto'
import { appendFileSync, readFileSync } from 'node:fs'
const args = process.argv.slice(2)
const method = args[args.indexOf('--method') + 1]
const endpoint = args.find(arg => arg.startsWith('repos/'))
const body = args.includes('--input') ? JSON.parse(readFileSync(0, 'utf8')) : null
appendFileSync(process.env.CAPTURE, JSON.stringify({ method, endpoint, body }) + '\\n')
const route = endpoint.replace(/^repos\\/[^/]+\\/[^/]+\\//, '')
const reply = value => console.log(JSON.stringify(value))
if (method === 'POST' && route === 'git/blobs') {
  const content = Buffer.from(body.content, 'base64')
  const hash = createHash('sha1').update('blob ' + content.length + '\\0')
  reply({ sha: hash.update(content).digest('hex') })
} else if (method === 'GET' && route.startsWith('git/commits/')) {
  reply({ sha: route.slice('git/commits/'.length), tree: { sha: 'base-tree' } })
} else if (method === 'POST' && route === 'git/trees') {
  reply({ sha: 'new-tree' })
} else if (method === 'POST' && route === 'git/commits') {
  reply({ sha: 'signed-oid', verification: process.env.UNVERIFIED
    ? { verified: false, reason: 'unsigned' }
    : { verified: true, reason: 'valid' } })
} else if (method === 'PATCH' && route.startsWith('git/refs/heads/')) {
  if (process.env.REF_REFUSED) {
    reply({ message: 'Update is not a fast forward', status: '422' })
    console.error('gh: Update is not a fast forward (HTTP 422)')
    process.exit(1)
  }
  reply({ ref: 'refs/' + route.slice('git/refs/'.length), object: { sha: body.sha } })
} else {
  process.exit(99)
}
`,
  )
  Object.assign(f.env, {
    PATH: `${join(f.root, 'bin')}:${process.env.PATH}`,
    GITHUB_REPOSITORY: 'Start9Labs/fixture',
    CAPTURE: f.capture,
  })
  f.runScript = () =>
    f.run('node', [SCRIPT, f.message, 'master', f.base], { encoding: 'utf8' })
  f.requests = () =>
    existsSync(f.capture)
      ? readFileSync(f.capture, 'utf8')
          .trim()
          .split('\n')
          .map(line => JSON.parse(line))
      : []
  f.request = (method, route) =>
    f
      .requests()
      .filter(
        request =>
          request.method === method &&
          request.endpoint === `repos/Start9Labs/fixture/${route}`,
      )
  f.blob = path => f.git('rev-parse', `:${path}`).trim()
  return f
}

const blobContent = request => Buffer.from(request.body.content, 'base64')

test('rename, large content and index ownership', t => {
  const f = commitFixture(t)
  f.write(
    'message',
    'chore: archive fragments\r\n\r\nDetails.\r\nMore details.\r\n',
  )
  f.git('mv', 'old.md', 'name with spaces.md')
  const contents = 'Large prose.\n'.repeat(20000)
  f.write('name with spaces.md', contents)
  f.git('add', 'name with spaces.md')
  f.write('name with spaces.md', 'unstaged content')
  const result = f.runScript()
  assert.equal(result.status, 0, result.stderr)
  assert.equal(result.stdout, 'signed-oid\n')
  assert.equal(
    f.request('POST', 'git/commits')[0].body.message,
    'chore: archive fragments\n\nDetails.\nMore details.\n',
  )
  const [blob] = f.request('POST', 'git/blobs')
  assert.equal(f.request('POST', 'git/blobs').length, 1)
  assert.deepEqual(blob.body, {
    content: Buffer.from(contents).toString('base64'),
    encoding: 'base64',
  })
  assert.equal(f.request('GET', `git/commits/${f.base}`).length, 1)
  assert.deepEqual(f.request('POST', 'git/trees')[0].body, {
    base_tree: 'base-tree',
    tree: [
      {
        path: 'name with spaces.md',
        mode: '100644',
        type: 'blob',
        sha: f.blob('name with spaces.md'),
      },
      { path: 'old.md', mode: '100644', type: 'blob', sha: null },
    ],
  })
  assert.deepEqual(f.request('PATCH', 'git/refs/heads/master')[0].body, {
    sha: 'signed-oid',
    force: false,
  })
  assert.deepEqual(
    f.requests().map(request => request.method),
    ['POST', 'GET', 'POST', 'POST', 'PATCH'],
  )
})

test('commit request carries no author, committer or signature', t => {
  const f = commitFixture(t)
  f.write('old.md', 'changed\n')
  f.git('add', 'old.md')
  const result = f.runScript()
  assert.equal(result.status, 0, result.stderr)
  assert.deepEqual(f.request('POST', 'git/commits')[0].body, {
    message: 'chore: archive fragments\n\nDetails.\n',
    tree: 'new-tree',
    parents: [f.base],
  })
})

test('staged binary blob over one MiB', t => {
  const f = commitFixture(t)
  const contents = Buffer.alloc(256 * 8192)
  for (let i = 0; i < contents.length; i++) contents[i] = i % 256
  f.write('large.bin', contents)
  f.git('add', 'large.bin')
  f.write('large.bin', Buffer.from('unstaged'))
  const result = f.runScript()
  assert.equal(result.status, 0, result.stderr)
  assert.equal(result.stdout, 'signed-oid\n')
  assert.deepEqual(blobContent(f.request('POST', 'git/blobs')[0]), contents)
  assert.equal(
    f.request('POST', 'git/trees')[0].body.tree[0].sha,
    f.blob('large.bin'),
  )
})

test('executables and symlinks keep their modes', t => {
  const f = commitFixture(t)
  f.write('tool.sh', '#!/bin/sh\n')
  symlinkSync('old.md', join(f.root, 'old-link'))
  f.git('add', '.')
  f.git('commit', '-qm', 'modes')
  f.base = f.git('rev-parse', 'HEAD').trim()
  chmodSync(join(f.root, 'tool.sh'), 0o755)
  f.write('run.sh', '#!/bin/sh\necho run\n')
  chmodSync(join(f.root, 'run.sh'), 0o755)
  symlinkSync('name.md', join(f.root, 'link'))
  rmSync(join(f.root, 'old-link'))
  rmSync(join(f.root, 'old.md'))
  symlinkSync('tool.sh', join(f.root, 'old.md'))
  f.git('add', '-A')
  const result = f.runScript()
  assert.equal(result.status, 0, result.stderr)
  assert.deepEqual(f.request('POST', 'git/trees')[0].body.tree, [
    { path: 'link', mode: '120000', type: 'blob', sha: f.blob('link') },
    { path: 'old-link', mode: '120000', type: 'blob', sha: null },
    { path: 'old.md', mode: '120000', type: 'blob', sha: f.blob('old.md') },
    { path: 'run.sh', mode: '100755', type: 'blob', sha: f.blob('run.sh') },
    { path: 'tool.sh', mode: '100755', type: 'blob', sha: f.blob('tool.sh') },
  ])
  assert.deepEqual(
    f.request('POST', 'git/blobs').map(blob => blobContent(blob).toString()),
    ['name.md', 'tool.sh', '#!/bin/sh\necho run\n', '#!/bin/sh\n'],
  )
})

test('deletion only', t => {
  const f = commitFixture(t)
  f.git('rm', '-q', 'old.md')
  const result = f.runScript()
  assert.equal(result.status, 0, result.stderr)
  assert.equal(f.request('POST', 'git/blobs').length, 0)
  assert.deepEqual(f.request('POST', 'git/trees')[0].body.tree, [
    { path: 'old.md', mode: '100644', type: 'blob', sha: null },
  ])
})

test('new gitlink is rejected', t => {
  const f = commitFixture(t)
  f.git('update-index', '--add', '--cacheinfo', `160000,${f.base},submodule`)
  const result = f.runScript()
  assert.equal(result.status, 2, result.stderr)
  assert.equal(existsSync(f.capture), false)
})

test('existing gitlink cannot be deleted or normalized', t => {
  const f = commitFixture(t)
  const originalBase = f.base
  for (const change of ['delete', 'normalize']) {
    f.git('reset', '-q', '--hard', originalBase)
    f.git(
      'update-index',
      '--add',
      '--cacheinfo',
      `160000,${originalBase},submodule`,
    )
    f.git('commit', '-qm', 'gitlink')
    f.base = f.git('rev-parse', 'HEAD').trim()
    f.git('update-index', '--force-remove', 'submodule')
    if (change === 'normalize') {
      f.write('submodule', 'ordinary file\n')
      f.git('add', 'submodule')
      rmSync(join(f.root, 'submodule'))
    }
    const result = f.runScript()
    assert.equal(result.status, 2, `${change}: ${result.stderr}`)
    assert.equal(existsSync(f.capture), false, change)
  }
})

test('unverified commit leaves the branch alone', t => {
  const f = commitFixture(t)
  f.write('old.md', 'changed\n')
  f.git('add', 'old.md')
  f.env.UNVERIFIED = '1'
  const result = f.runScript()
  assert.equal(result.status, 3, result.stderr)
  assert.equal(result.stdout, '')
  assert.match(result.stderr, /did not verify signed-oid.*unsigned/)
  assert.equal(f.request('POST', 'git/commits').length, 1)
  assert.equal(f.request('PATCH', 'git/refs/heads/master').length, 0)
})

test('refused ref update fails with the response', t => {
  const f = commitFixture(t)
  f.write('old.md', 'changed\n')
  f.git('add', 'old.md')
  f.env.REF_REFUSED = '1'
  const result = f.runScript()
  assert.equal(result.status, 1, result.stderr)
  assert.equal(result.stdout, '')
  assert.match(result.stderr, /"message":"Update is not a fast forward"/)
  assert.match(result.stderr, /HTTP 422/)
})

const LOCAL_API = String.raw`#!/usr/bin/env node
import { spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const args = process.argv.slice(2)
const remote = process.env.LOCAL_REMOTE
const reply = value => console.log(JSON.stringify(value))
function refuse(message) {
  reply({ message, status: '422' })
  console.error('gh: ' + message + ' (HTTP 422)')
  process.exit(1)
}
function git(gitArgs, { input, env = process.env, check = true } = {}) {
  const result = spawnSync('git', ['--git-dir', remote, ...gitArgs], { env, input, maxBuffer: Infinity })
  if (result.error) throw result.error
  if (check && result.status !== 0) throw new Error(result.stderr.toString())
  return result.status === 0 ? result.stdout.toString().trim() : null
}
function writeTree(base, entries) {
  const temp = mkdtempSync(join(tmpdir(), 'local-github-api-'))
  const env = { ...process.env, GIT_INDEX_FILE: join(temp, 'index'), GIT_WORK_TREE: temp }
  try {
    git(['read-tree', base], { env })
    for (const { path, mode, sha } of entries) {
      if (sha === null) git(['update-index', '--force-remove', path], { env })
      else git(['update-index', '--add', '--cacheinfo', mode + ',' + sha + ',' + path], { env })
    }
    return git(['write-tree'], { env })
  } finally {
    rmSync(temp, { recursive: true, force: true })
  }
}
function commit(tree, parents, message) {
  return git(['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.com',
    '-c', 'commit.gpgsign=false', 'commit-tree', tree, ...parents.flatMap(p => ['-p', p])],
    { input: Buffer.from(message) })
}

if (args[0] === 'pr' && args[1] === 'create') {
  const body = readFileSync(args[args.indexOf('--body-file') + 1], 'utf8')
  writeFileSync(process.env.PR_CAPTURE, JSON.stringify({ args, body }))
  process.exit(0)
}
if (args.some(arg => arg.endsWith('/releases/assets/1'))) {
  process.stdout.write(process.env.PUBLISHED_NOTES)
  process.exit(0)
}
if (args.some(arg => arg.endsWith('/releases'))) {
  console.log('start-sdk/v1.2.3\t1')
  process.exit(0)
}
const option = args.findIndex(arg => arg === '--method' || arg === '-X')
const method = option === -1 ? 'GET' : args[option + 1]
const route = args.find(arg => arg.startsWith('repos/')).replace(/^repos\/[^/]+\/[^/]+\//, '')
let body = {}
if (args.includes('--input')) body = JSON.parse(readFileSync(0, 'utf8'))
args.forEach((arg, index) => {
  if (arg !== '-f' && arg !== '-F') return
  const [key, ...value] = args[index + 1].split('=')
  body[key] = arg === '-F' ? JSON.parse(value.join('=')) : value.join('=')
})

if (method === 'POST' && route === 'git/blobs') {
  reply({ sha: git(['hash-object', '-w', '--stdin'], { input: Buffer.from(body.content, 'base64') }) })
} else if (method === 'GET' && route.startsWith('git/commits/')) {
  const sha = route.slice('git/commits/'.length)
  reply({ sha, tree: { sha: git(['rev-parse', sha + '^{tree}']) } })
} else if (method === 'POST' && route === 'git/trees') {
  reply({ sha: writeTree(body.base_tree, body.tree) })
} else if (method === 'POST' && route === 'git/commits') {
  const verified = !process.env.UNVERIFIED
  reply({ sha: commit(body.tree, body.parents, body.message),
    verification: { verified, reason: verified ? 'valid' : 'unsigned' } })
} else if (method === 'POST' && route === 'git/refs') {
  if (git(['rev-parse', '--verify', '-q', body.ref], { check: false })) refuse('Reference already exists')
  git(['update-ref', body.ref, body.sha, ''])
  reply({ ref: body.ref, object: { sha: body.sha } })
} else if (method === 'PATCH' && route.startsWith('git/refs/heads/')) {
  const ref = 'refs/' + route.slice('git/refs/'.length)
  const marker = join(process.env.RUNNER_TEMP, 'raced')
  if (process.env.RACE && !body.force && !existsSync(marker)) {
    writeFileSync(marker, '')
    const head = git(['rev-parse', ref])
    const blob = git(['hash-object', '-w', '--stdin'],
      { input: '---\nrelease-notes: false\n---\n\n- New work after the tag.\n' })
    const tree = writeTree(head, [{ path: 'projects/start-sdk/changelog/patch-fixed-after-tag.md', mode: '100644', sha: blob }])
    git(['update-ref', ref, commit(tree, [head], 'concurrent work'), head])
  }
  const current = git(['rev-parse', ref])
  if (!body.force && git(['merge-base', '--is-ancestor', current, body.sha], { check: false }) === null) {
    refuse('Update is not a fast forward')
  }
  git(['update-ref', ref, body.sha, current])
  reply({ ref, object: { sha: body.sha } })
} else {
  console.error('unexpected gh call: ' + args.join(' '))
  process.exit(99)
}
`

function localApi(f, remote) {
  const mock = f.write('bin/gh.mjs', LOCAL_API)
  chmodSync(mock, 0o755)
  f.env.LOCAL_REMOTE = remote
  f.env.GITHUB_REPOSITORY = 'fixture/repo'
}

function workflowStep(workflow, name) {
  const lines = readFileSync(join(REPO, '.github/workflows', workflow), 'utf8')
    .split(`      - name: ${name}\n`)[1]
    .split('        run: |\n')[1]
    .split('\n')
  const end = lines.findIndex(line => line && !line.startsWith(' '.repeat(10)))
  return lines
    .slice(0, end === -1 ? undefined : end)
    .map(line => line.slice(10))
    .join('\n')
}

test('tag compiler workflow with local commit API', t => {
  const f = commitFixture(t)
  mkdirSync(join(f.root, 'scripts'))
  for (const name of [
    'changelog.mjs',
    'changelog-version.mjs',
    'commit-staged.mjs',
    'docs-versions.mjs',
  ]) {
    copyFileSync(join(REPO, 'scripts', name), join(f.root, 'scripts', name))
  }
  copyFileSync(join(REPO, '.prettierrc.json'), join(f.root, '.prettierrc.json'))
  const pin = JSON.parse(readFileSync(join(REPO, 'package.json'), 'utf8'))
    .devDependencies.prettier
  f.write(
    'package.json',
    JSON.stringify({ devDependencies: { prettier: pin } }),
  )
  f.write(
    'projects/start-sdk/changelog/patch-fixed-released.md',
    '---\nrelease-notes: true\n---\n\n- Released fix.\n\n- Second released fix.\n',
  )
  f.write(
    'projects/start-sdk/changelog/patch-security-released.md',
    '---\nrelease-notes: true\n---\n\n- Released security fix.\n',
  )
  f.write(
    'projects/start-sdk/changelog/patch-fixed-second-released.md',
    '---\nrelease-notes: false\n---\n\n- Third released fix.\n',
  )
  f.write('projects/start-sdk/CHANGELOG.md', '# Changelog\n')
  f.git('add', 'scripts', 'projects', 'package.json', '.prettierrc.json')
  f.git('commit', '-qm', 'release')
  f.git('-c', 'tag.gpgsign=false', 'tag', 'start-sdk/v1.2.3')
  const catalog = 'shared-libs/crates/start-core/locales/release-notes.json'
  f.write(catalog, readFileSync(join(REPO, catalog)))
  f.git('add', 'shared-libs')
  f.git('commit', '-qm', 'compiler labels')
  const remote = join(f.root, 'origin.git')
  f.git('init', '--bare', '-q', remote)
  f.git('remote', 'add', 'origin', remote)
  f.git('push', '-q', 'origin', 'HEAD:master', 'HEAD:live-docs', '--tags')
  f.git('fetch', '-q', 'origin')
  f.git('checkout', '--detach', 'origin/live-docs')
  localApi(f, remote)
  const workflow = readFileSync(
    join(REPO, '.github/workflows/docs-sync-on-tag.yml'),
    'utf8',
  )
  const shell = workflowStep(
    'docs-sync-on-tag.yml',
    'Compile released changelogs onto master',
  )
  const runnerTemp = join(f.root, 'runner-temp')
  mkdirSync(runnerTemp)
  const published = {
    en_US:
      '## Highlights\n\n- Released fix, reworded.\n\n## Security\n\n- Released security fix.\n',
    pl_PL:
      '## Nowości\n\n- Poprawka.\n\n## Bezpieczeństwo\n\n- Poprawka bezpieczeństwa.\n',
  }
  Object.assign(f.env, {
    PUBLISHED_NOTES: JSON.stringify(published),
    RACE: '1',
    RUNNER_TEMP: runnerTemp,
    GITHUB_REF_NAME: 'start-sdk/v1.2.3',
  })
  let result = f.run('bash', ['-c', shell], { encoding: 'utf8' })
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.equal(f.git('status', '--porcelain', '--untracked-files=no'), '')
  assert.equal(
    f.git('rev-parse', 'HEAD'),
    f.git('rev-parse', 'origin/live-docs'),
  )
  const history = f.git('show', 'origin/master:projects/start-sdk/CHANGELOG.md')
  assert.equal(
    history,
    '# Changelog\n\n## [1.2.3]\n\n### Fixed\n\n- Released fix.\n\n- Second released fix.\n\n- Third released fix.\n\n### Security\n\n- Released security fix.\n',
  )
  assert.ok(
    !f
      .git('ls-tree', '-r', '--name-only', 'origin/master', 'projects')
      .includes('patch-fixed-released.md'),
  )
  assert.match(
    f.git(
      'show',
      'origin/master:projects/start-sdk/changelog/patch-fixed-after-tag.md',
    ),
    /New work after the tag\./,
  )
  assert.ok(!history.includes('New work after the tag.'))
  assert.equal(
    f.git('log', '-1', '--format=%B', 'origin/master'),
    'chore: compile released changelog fragments\n\n',
  )
  const notesPath = 'projects/start-sdk/release-notes/1.2.3.md'
  const notes = f.git('show', `origin/master:${notesPath}`)
  assert.equal(notes, published.en_US)
  assert.match(result.stderr, /Update is not a fast forward/)
  const localizedPath = 'projects/start-sdk/release-notes/1.2.3.pl_PL.md'
  const localized = f.git('show', `origin/master:${localizedPath}`)
  assert.equal(localized, published.pl_PL)
  assert.ok(existsSync(join(runnerTemp, 'scripts/changelog.mjs')))
  assert.deepEqual(
    readFileSync(join(runnerTemp, catalog)),
    readFileSync(join(REPO, catalog)),
  )
  assert.equal(
    f.git('ls-tree', '-r', '--name-only', 'start-sdk/v1.2.3', '--', catalog),
    '',
  )
  assert.ok(
    !f
      .git(
        'ls-tree',
        '-r',
        '--name-only',
        'origin/master',
        'projects/start-sdk/changelog',
      )
      .includes('.pl_PL.md'),
  )
  const master = f.git('rev-parse', 'origin/master')
  result = f.run('bash', ['-c', shell], { encoding: 'utf8' })
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.equal(f.git('rev-parse', 'origin/master'), master)
  assert.equal(
    f.git('show', 'origin/master:projects/start-sdk/CHANGELOG.md'),
    history,
  )
  assert.equal(f.git('show', `origin/master:${notesPath}`), notes)
  f.git('read-tree', '-u', '--reset', 'start-sdk/v1.2.3')
  f.env.PROJECT = 'start-sdk'
  const archive = workflow
    .slice(
      workflow.indexOf(
        '          git ls-tree -r --name-only origin/master -- projects/',
      ),
      workflow.indexOf('          git add -A'),
    )
    .split('\n')
    .map(line => line.slice(10))
    .join('\n')
  const copied = f.run('bash', ['-euo', 'pipefail', '-c', archive], {
    encoding: 'utf8',
  })
  assert.equal(copied.status, 0, copied.stdout + copied.stderr)
  assert.equal(f.git('show', `:${notesPath}`), notes)
  assert.equal(f.git('show', `:${localizedPath}`), localized)
})

function backportFixture(t, { conflict }) {
  const f = commitFixture(t)
  mkdirSync(join(f.root, 'scripts'))
  copyFileSync(SCRIPT, join(f.root, 'scripts/commit-staged.mjs'))
  f.write('projects/start-docs/build.sh', '#!/bin/sh\necho build\n')
  chmodSync(join(f.root, 'projects/start-docs/build.sh'), 0o755)
  f.write('projects/start-docs/src/page.md', 'Published text.\n')
  f.git('add', 'scripts', 'projects')
  f.git('commit', '-qm', 'docs')
  const remote = join(f.root, 'origin.git')
  f.git('init', '--bare', '-q', remote)
  f.git('remote', 'add', 'origin', remote)
  f.git('push', '-q', 'origin', 'HEAD:master', 'HEAD:live-docs')
  const before = f.git('rev-parse', 'HEAD').trim()
  if (conflict) {
    f.write('projects/start-docs/src/page.md', 'Unreleased text.\n')
    f.git('commit', '-qam', 'master edit')
    f.git('push', '-q', 'origin', 'HEAD:master')
    f.git('reset', '-q', '--hard', before)
  }
  f.write('projects/start-docs/build.sh', '#!/bin/sh\necho fixed build\n')
  f.write('projects/start-docs/src/page.md', 'Corrected text.\n')
  f.git('commit', '-qam', 'docs: fix the build')
  f.git('push', '-q', 'origin', 'HEAD:live-docs')
  const after = f.git('rev-parse', 'HEAD').trim()
  localApi(f, remote)
  f.prCapture = join(f.root, 'pr.json')
  Object.assign(f.env, {
    BEFORE: before,
    AFTER: after,
    PR_CAPTURE: f.prCapture,
  })
  const runnerTemp = join(f.root, 'runner-temp')
  mkdirSync(runnerTemp)
  let runs = 0
  f.runStep = (name, env = {}) => {
    runs += 1
    const work = join(f.root, `work-${runs}`)
    f.git('clone', '-q', '--branch', 'master', remote, work)
    f.git('-C', work, 'config', 'commit.gpgsign', 'false')
    f.git('-C', work, 'config', 'user.name', 'Fixture')
    f.git('-C', work, 'config', 'user.email', 'fixture@example.com')
    const output = join(runnerTemp, `output-${runs}`)
    const result = f.run(
      'bash',
      ['-c', workflowStep('docs-backport.yml', name)],
      {
        cwd: work,
        encoding: 'utf8',
        env: {
          ...f.env,
          RUNNER_TEMP: runnerTemp,
          GITHUB_OUTPUT: output,
          ...env,
        },
      },
    )
    assert.equal(result.status, 0, result.stdout + result.stderr)
    return existsSync(output) ? readFileSync(output, 'utf8') : ''
  }
  f.remote = (...args) => f.git('--git-dir', remote, ...args).trim()
  f.prBody = () => JSON.parse(readFileSync(f.prCapture, 'utf8')).body
  return f
}

test('docs backport lands an executable file on master', t => {
  const f = backportFixture(t, { conflict: false })
  const master = f.remote('rev-parse', 'master')
  assert.equal(f.runStep('Backport onto master'), '')
  assert.equal(f.remote('rev-parse', 'master^'), master)
  assert.match(
    f.remote('ls-tree', 'master', 'projects/start-docs/build.sh'),
    /^100755 /,
  )
  assert.equal(
    f.remote('show', 'master:projects/start-docs/build.sh'),
    '#!/bin/sh\necho fixed build',
  )
  assert.match(
    f.remote('log', '-1', '--format=%B', 'master'),
    /^docs: backport live-docs [0-9a-f]+ to master\n\nBackports the following from live-docs:\n\n- [0-9a-f]+ docs: fix the build$/,
  )
})

test('docs backport conflict opens a signed branch', t => {
  const f = backportFixture(t, { conflict: true })
  const master = f.remote('rev-parse', 'master')
  const reason = f.runStep('Backport onto master')
  assert.equal(reason, 'fallback=conflict\n')
  const branch = `backport/live-docs-${f.env.AFTER.slice(0, 7)}`
  f.runStep('Open a PR instead', { REASON: 'conflict' })
  assert.equal(f.remote('rev-parse', `${branch}^`), master)
  assert.match(
    f.remote('ls-tree', branch, 'projects/start-docs/build.sh'),
    /^100755 /,
  )
  assert.match(
    f.remote('show', `${branch}:projects/start-docs/src/page.md`),
    /^<<<<<<< /,
  )
  assert.match(
    f.remote('log', '-1', '--format=%B', branch),
    /^docs: backport live-docs [0-9a-f]+ to master\n/,
  )
  let body = f.prBody()
  assert.match(body, /resolve the conflicts before merging/)
  assert.match(body, /signed by the docs bot/)
  assert.doesNotMatch(body, /must replace them/)
  assert.match(body, /`projects\/start-docs\/src\/page.md`/)

  f.runStep('Open a PR instead', { REASON: 'unverified', UNVERIFIED: '1' })
  assert.equal(f.remote('rev-parse', `${branch}^`), master)
  assert.match(
    f.remote('log', '-1', '--format=%B', branch),
    /^docs: fix the build\n[^]*\(cherry picked from commit /,
  )
  body = f.prBody()
  assert.match(body, /did not verify the signature/)
  assert.match(body, /must replace them with a signed commit/)
  assert.doesNotMatch(body, /signed by the docs bot/)
})

test('empty index makes no request', t => {
  const f = commitFixture(t)
  assert.equal(f.runScript().status, 0)
  assert.equal(existsSync(f.capture), false)
})
