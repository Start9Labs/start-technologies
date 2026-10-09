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
  f.capture = join(f.root, 'payload.json')
  f.message = f.write('message', 'chore: archive fragments\n\nDetails.\n')
  f.nodeExecutable(
    'bin/gh',
    `import { readFileSync, writeFileSync } from 'node:fs'
writeFileSync(process.env.CAPTURE, readFileSync(0))
console.log(JSON.stringify(process.env.FAIL
  ? { errors: [{ message: 'head moved' }] }
  : { data: { createCommitOnBranch: { commit: { oid: 'signed-oid' } } } }))
`,
  )
  Object.assign(f.env, {
    PATH: `${join(f.root, 'bin')}:${process.env.PATH}`,
    GITHUB_REPOSITORY: 'Start9Labs/fixture',
    CAPTURE: f.capture,
  })
  f.runScript = () =>
    f.run('node', [SCRIPT, f.message, 'master', f.base], { encoding: 'utf8' })
  f.payload = () => JSON.parse(readFileSync(f.capture, 'utf8')).variables.input
  return f
}

test('rename, large content and index ownership', t => {
  const f = commitFixture(t)
  f.git('mv', 'old.md', 'name with spaces.md')
  const contents = 'Large prose.\n'.repeat(20000)
  f.write('name with spaces.md', contents)
  f.git('add', 'name with spaces.md')
  f.write('name with spaces.md', 'unstaged content')
  const result = f.runScript()
  assert.equal(result.status, 0, result.stderr)
  const payload = f.payload()
  assert.equal(payload.expectedHeadOid, f.base)
  assert.deepEqual(payload.branch, {
    repositoryNameWithOwner: 'Start9Labs/fixture',
    branchName: 'master',
  })
  assert.deepEqual(payload.message, {
    headline: 'chore: archive fragments',
    body: 'Details.',
  })
  assert.deepEqual(payload.fileChanges.deletions, [{ path: 'old.md' }])
  const [addition] = payload.fileChanges.additions
  assert.equal(payload.fileChanges.additions.length, 1)
  assert.equal(addition.path, 'name with spaces.md')
  assert.equal(Buffer.from(addition.contents, 'base64').toString(), contents)
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
  const additions = f.payload().fileChanges.additions
  assert.equal(additions[0].path, 'large.bin')
  assert.deepEqual(Buffer.from(additions[0].contents, 'base64'), contents)
})

test('modes are not silently rewritten', t => {
  const f = commitFixture(t)
  for (const mode of ['symlink', 'executable']) {
    f.git('reset', '--hard', f.base)
    const path = join(f.root, 'unsupported')
    rmSync(path, { force: true })
    if (mode === 'symlink') symlinkSync('old.md', path)
    else {
      f.write('unsupported', '#!/bin/sh\n')
      chmodSync(path, 0o755)
    }
    f.git('add', 'unsupported')
    assert.equal(f.runScript().status, 2, mode)
    assert.equal(existsSync(f.capture), false, mode)
  }
})

test('deletion only', t => {
  const f = commitFixture(t)
  f.git('rm', 'old.md')
  const result = f.runScript()
  assert.equal(result.status, 0, result.stderr)
  assert.deepEqual(f.payload().fileChanges, {
    additions: [],
    deletions: [{ path: 'old.md' }],
  })
})

test('existing unsupported modes cannot be deleted or normalized', t => {
  const f = commitFixture(t)
  const originalBase = f.base
  for (const mode of ['symlink', 'executable', 'gitlink']) {
    for (const change of ['delete', 'normalize']) {
      f.git('reset', '--hard', originalBase)
      const path = join(f.root, 'unsupported')
      rmSync(path, { force: true })
      if (mode === 'gitlink') {
        f.git(
          'update-index',
          '--add',
          '--cacheinfo',
          `160000,${originalBase},unsupported`,
        )
      } else {
        if (mode === 'symlink') symlinkSync('old.md', path)
        else {
          f.write('unsupported', '#!/bin/sh\n')
          chmodSync(path, 0o755)
        }
        f.git('add', 'unsupported')
      }
      f.git('commit', '-qm', 'unsupported mode')
      f.base = f.git('rev-parse', 'HEAD').trim()
      rmSync(path, { force: true })
      f.git('update-index', '--force-remove', 'unsupported')
      if (change === 'normalize') {
        f.write('unsupported', 'ordinary file\n')
        f.git('add', 'unsupported')
      }
      const result = f.runScript()
      assert.equal(result.status, 2, `${mode}/${change}: ${result.stderr}`)
      assert.equal(existsSync(f.capture), false, `${mode}/${change}`)
    }
  }
})

test('new gitlink is rejected', t => {
  const f = commitFixture(t)
  f.git('update-index', '--add', '--cacheinfo', `160000,${f.base},submodule`)
  const result = f.runScript()
  assert.equal(result.status, 2, result.stderr)
  assert.equal(existsSync(f.capture), false)
})

test('GraphQL errors fail even with HTTP success', t => {
  const f = commitFixture(t)
  f.write('old.md', 'changed\n')
  f.git('add', 'old.md')
  f.env.FAIL = '1'
  const result = f.runScript()
  assert.notEqual(result.status, 0)
  assert.match(result.stderr, /head moved/)
})

test('tag compiler workflow with local commit API', t => {
  const f = commitFixture(t)
  mkdirSync(join(f.root, 'scripts'))
  for (const name of [
    'changelog.mjs',
    'changelog-version.mjs',
    'commit-staged.mjs',
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
    '- Released fix.\n\n- Second released fix.\n',
  )
  f.write(
    'projects/start-sdk/changelog/patch-fixed-second-released.md',
    '- Third released fix.\n',
  )
  f.write('projects/start-sdk/CHANGELOG.md', '# Changelog\n')
  f.git('add', 'scripts', 'projects', 'package.json', '.prettierrc.json')
  f.git('commit', '-qm', 'release')
  f.git('-c', 'tag.gpgsign=false', 'tag', 'start-sdk/v1.2.3')
  const remote = join(f.root, 'origin.git')
  f.git('init', '--bare', '-q', remote)
  f.git('remote', 'add', 'origin', remote)
  f.git('push', '-q', 'origin', 'HEAD:master', 'HEAD:live-docs', '--tags')
  f.git('fetch', '-q', 'origin')
  f.git('checkout', '--detach', 'origin/live-docs')
  const mock = join(f.root, 'bin/gh.mjs')
  f.write(
    'bin/gh.mjs',
    String.raw`#!/usr/bin/env node
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const payload = JSON.parse(readFileSync(0, 'utf8')).variables.input
const remote = process.env.LOCAL_REMOTE
const env = { ...process.env }
const marker = join(process.env.RUNNER_TEMP, 'raced')
const race = !existsSync(marker)
if (race) {
  writeFileSync(marker, '')
  payload.fileChanges = { deletions: [], additions: [{
    path: 'projects/start-sdk/changelog/patch-fixed-after-tag.md',
    contents: Buffer.from('- New work after the tag.\n').toString('base64'),
  }] }
}
function git(args, input) {
  const result = spawnSync('git', ['--git-dir', remote, ...args], { env, input, maxBuffer: Infinity })
  assert.ifError(result.error)
  assert.equal(result.status, 0, result.stderr.toString())
  return result.stdout.toString().trim()
}
const temp = mkdtempSync(join(tmpdir(), 'local-commit-api-'))
let oid
try {
  env.GIT_INDEX_FILE = join(temp, 'index')
  env.GIT_WORK_TREE = temp
  git(['read-tree', payload.expectedHeadOid])
  for (const deletion of payload.fileChanges.deletions) {
    git(['update-index', '--force-remove', deletion.path])
  }
  for (const addition of payload.fileChanges.additions) {
    const blob = git(['hash-object', '-w', '--stdin'], Buffer.from(addition.contents, 'base64'))
    git(['update-index', '--add', '--cacheinfo', '100644,' + blob + ',' + addition.path])
  }
  const tree = git(['write-tree'])
  oid = git(['-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.com',
    '-c', 'commit.gpgsign=false', 'commit-tree', tree, '-p', payload.expectedHeadOid],
    Buffer.from(payload.message.headline))
  git(['update-ref', 'refs/heads/master', oid, payload.expectedHeadOid])
} finally {
  rmSync(temp, { recursive: true, force: true })
}
console.log(JSON.stringify(race
  ? { errors: [{ message: 'head moved' }] }
  : { data: { createCommitOnBranch: { commit: { oid } } } }))
`,
  )
  chmodSync(mock, 0o755)
  const workflow = readFileSync(
    join(REPO, '.github/workflows/docs-sync-on-tag.yml'),
    'utf8',
  )
  const block = workflow
    .split('      - name: Compile released changelogs onto master\n')[1]
    .split('        run: |\n')[1]
    .split('\n      - name:')[0]
  const shell = block
    .split('\n')
    .map(line => line.slice(10))
    .join('\n')
  const runnerTemp = join(f.root, 'runner-temp')
  mkdirSync(runnerTemp)
  Object.assign(f.env, {
    LOCAL_REMOTE: remote,
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
    '# Changelog\n\n## [1.2.3]\n\n### Fixed\n\n- Released fix.\n\n- Second released fix.\n\n- Third released fix.\n',
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
  assert.match(result.stderr, /head moved/)
  const master = f.git('rev-parse', 'origin/master')
  result = f.run('bash', ['-c', shell], { encoding: 'utf8' })
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.equal(f.git('rev-parse', 'origin/master'), master)
  assert.equal(
    f.git('show', 'origin/master:projects/start-sdk/CHANGELOG.md'),
    history,
  )
})

test('empty index makes no request', t => {
  const f = commitFixture(t)
  assert.equal(f.runScript().status, 0)
  assert.equal(existsSync(f.capture), false)
})
