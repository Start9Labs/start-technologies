import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { syncNext } from '../sync-next/sync-next.mjs'

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'sync-next-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const remote = join(root, 'remote.git')
  const cwd = join(root, 'work')
  const env = {
    ...process.env,
    GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_CONFIG_SYSTEM: '/dev/null',
  }
  const command = (dir, ...args) =>
    execFileSync('git', args, {
      cwd: dir,
      env,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim()
  command(root, 'init', '--bare', '--initial-branch=master', remote)
  command(root, 'clone', remote, cwd)
  const git = (...args) => command(cwd, ...args)
  git('config', 'user.name', 'Fixture')
  git('config', 'user.email', 'fixture@example.org')
  git('config', 'commit.gpgsign', 'false')
  const commit = (file, text) => {
    writeFileSync(join(cwd, file), text)
    git('add', file)
    git('commit', '-m', text)
    return git('rev-parse', 'HEAD')
  }
  const initial = commit('shared', 'initial')
  git('push', 'origin', 'master')
  const pullRequests = []
  const forge = {
    defaultBranch: async () => 'master',
    hasOpenPullRequest: async () => false,
    createPullRequest: async body => {
      pullRequests.push(body)
    },
  }
  const run = () =>
    syncNext({
      cwd,
      base: 'master',
      forge,
      botName: 'CI bot',
      botEmail: 'ci@example.org',
    })
  const remoteSha = branch =>
    command(root, '--git-dir', remote, 'rev-parse', branch)
  return { git, commit, initial, forge, pullRequests, run, remoteSha }
}

test('creates next at the base tip when no iteration branch exists', async t => {
  const f = fixture(t)
  assert.equal(await f.run(), 'created')
  assert.equal(f.remoteSha('next'), f.initial)
  assert.equal(f.pullRequests.length, 0)
})

test('fast-forwards next without rewriting the base', async t => {
  const f = fixture(t)
  f.git('push', 'origin', 'master:next')
  const tip = f.commit('base', 'base change')
  f.git('push', 'origin', 'master')
  assert.equal(await f.run(), 'fast-forwarded')
  assert.equal(f.remoteSha('next'), tip)
  assert.equal(f.remoteSha('master'), tip)
})

test('uses explicit next/base even for the default branch', async t => {
  const f = fixture(t)
  f.git('push', 'origin', 'master:next/master')
  const tip = f.commit('base', 'base change')
  f.git('push', 'origin', 'master')
  await f.run()
  assert.equal(f.remoteSha('next/master'), tip)
})

test('non-default branches get their own iteration branch', async t => {
  const f = fixture(t)
  f.forge.defaultBranch = async () => 'main'
  assert.equal(await f.run(), 'created')
  assert.equal(f.remoteSha('next/master'), f.initial)
})

test('leaves an iteration branch containing the base untouched', async t => {
  const f = fixture(t)
  f.git('checkout', '-b', 'next')
  const tip = f.commit('iteration', 'iteration change')
  f.git('push', 'origin', 'next')
  assert.equal(await f.run(), 'unchanged')
  assert.equal(f.remoteSha('next'), tip)
  assert.equal(f.remoteSha('master'), f.initial)
})

function diverge(f, conflicting) {
  f.git('checkout', '-b', 'next')
  const nextTip = f.commit(
    conflicting ? 'shared' : 'iteration',
    'iteration change',
  )
  f.git('push', 'origin', 'next')
  f.git('checkout', 'master')
  const baseTip = f.commit(conflicting ? 'shared' : 'base', 'base change')
  f.git('push', 'origin', 'master')
  return { nextTip, baseTip }
}

test('merges diverged branches while preserving both histories', async t => {
  const f = fixture(t)
  const { nextTip, baseTip } = diverge(f, false)
  assert.equal(await f.run(), 'merged')
  const parents = f
    .git('rev-list', '--parents', '-n', '1', f.remoteSha('next'))
    .split(' ')
    .slice(1)
  assert.deepEqual(parents, [nextTip, baseTip])
  assert.equal(f.remoteSha('master'), baseTip)
})

test('conflicts create a scratch-headed pull request without changing base or next', async t => {
  const f = fixture(t)
  const { nextTip, baseTip } = diverge(f, true)
  assert.equal(await f.run(), 'pull-request-created')
  assert.equal(f.remoteSha('master'), baseTip)
  assert.equal(f.remoteSha('next'), nextTip)
  assert.equal(f.remoteSha('sync-next/master'), baseTip)
  assert.equal(f.pullRequests[0].base, 'next')
  assert.equal(f.pullRequests[0].head, 'sync-next/master')
})

test('an existing sync pull request keeps its conflict-resolution commits', async t => {
  const f = fixture(t)
  diverge(f, true)
  f.git('push', 'origin', `${f.initial}:refs/heads/sync-next/master`)
  f.forge.hasOpenPullRequest = async (base, head) => {
    assert.deepEqual([base, head], ['next', 'sync-next/master'])
    return true
  }
  assert.equal(await f.run(), 'pull-request-exists')
  assert.equal(f.remoteSha('sync-next/master'), f.initial)
  assert.equal(f.pullRequests.length, 0)
})

test('failure to create a conflict pull request fails the sync', async t => {
  const f = fixture(t)
  const { nextTip, baseTip } = diverge(f, true)
  f.forge.createPullRequest = async () => {
    throw new Error('API denied')
  }
  await assert.rejects(f.run(), /API denied/)
  assert.equal(f.remoteSha('next'), nextTip)
  assert.equal(f.remoteSha('master'), baseTip)
})
