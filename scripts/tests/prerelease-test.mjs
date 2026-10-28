import assert from 'node:assert/strict'
import { copyFileSync, existsSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { fixture, REPO } from './fixtures.mjs'

const VERSION = '0.4.0.1'
const PREFIX = `start-os/v${VERSION}`
const ALPHA = 'https://alpha-registry-x.start9.com'
const BETA = 'https://beta-registry.start9.com'

function prereleaseFixture(t) {
  const f = fixture(t)
  const { root, env, write, git, run, nodeExecutable } = f
  git('init', '-q', '-b', 'master')
  git('config', 'user.name', 'Fixture')
  git('config', 'user.email', 'fixture@example.com')
  git('config', 'commit.gpgsign', 'false')
  git('config', 'tag.gpgsign', 'false')
  git('remote', 'add', 'origin', root)
  for (const name of [
    'manage-release.sh',
    'changelog.mjs',
    'changelog-version.mjs',
  ]) {
    write(`scripts/${name}`, '')
    copyFileSync(join(REPO, 'scripts', name), join(root, 'scripts', name))
  }
  const labels = 'shared-libs/crates/start-core/locales/release-notes.json'
  write(labels, readFileSync(join(REPO, labels)))
  write(
    'projects/start-os/changelog/patch-fixed-localized.md',
    '---\nuser-facing: true\n---\n\n- Visible correction.\n',
  )
  write(
    'projects/start-os/changelog/patch-fixed-localized.pl_PL.md',
    '- Widoczna poprawka.\n',
  )
  write(
    'package.json',
    JSON.stringify({
      version: VERSION,
      devDependencies: JSON.parse(readFileSync(join(REPO, 'package.json')))
        .devDependencies,
    }),
  )
  copyFileSync(join(REPO, '.prettierrc.json'), write('.prettierrc.json', ''))
  write('projects/start-os/Cargo.toml', '[package]\nversion = "0.4.0-rev.1"\n')
  write(
    'projects/start-os/docs/src/installing.md',
    `https://github.com/Start9Labs/start-technologies/releases/tag/${PREFIX}\n`,
  )
  write(
    'projects/start-os/changelog/patch-security-example.md',
    '---\nuser-facing: false\n---\n\n- Security correction.\n',
  )
  function commit() {
    git(
      'add',
      'scripts',
      'package.json',
      '.prettierrc.json',
      'projects',
      'shared-libs',
    )
    git('commit', '--allow-empty', '-qm', 'fixture')
    return git('rev-parse', 'HEAD').trim()
  }
  const imageCommit = commit()
  Object.assign(env, {
    HOME: join(root, 'home'),
    FIXTURE_ROOT: root,
    PATH: `${join(root, 'bin')}:${env.PATH}`,
  })
  write('home/.startos/id.key.pem', 'fixture')
  const index = hash => ({
    versions: {
      [VERSION]: {
        gitHash: hash,
        sourceVersion: `>=0.3.5 <=${VERSION}`,
        iso: { x86_64: { urls: [`https://images/${hash}.iso`] } },
        squashfs: { x86_64: { urls: [`https://images/${hash}.squashfs`] } },
      },
    },
  })
  function alpha(hash) {
    write('alpha.json', JSON.stringify(index(hash)))
  }
  alpha(imageCommit)
  write('beta.json', '{"versions":{}}')
  nodeExecutable(
    'bin/start-cli',
    `
import { readFileSync, writeFileSync, appendFileSync } from 'node:fs'
import { join } from 'node:path'
const root = process.env.FIXTURE_ROOT
const args = process.argv.slice(2)
const read = name => JSON.parse(readFileSync(join(root, name + '.json')))
const write = (name, data) => writeFileSync(join(root, name + '.json'), JSON.stringify(data))
const flag = name => args[args.indexOf(name) + 1]
const registry = args.find(arg => arg.startsWith('--registry='))?.slice(11)
const channel = registry === ${JSON.stringify(ALPHA)} ? 'alpha' : 'beta'
if (args.at(-1) === '--help') console.log('<RELEASE_NOTES_OR_JSON>')
else if (args.includes('index')) console.log(JSON.stringify(read(channel)))
else if (args.includes('promote')) {
  appendFileSync(join(root, 'promotions'), 'promote\\n')
  if (process.env.FAIL_PROMOTE === '1') process.exit(41)
  if (flag('--expected-git-hash') !== read('alpha').versions[${JSON.stringify(VERSION)}].gitHash) throw new Error('promoted hash mismatch')
  write('beta', read('alpha'))
} else if (args.includes('remove')) {
  appendFileSync(join(root, 'removals'), 'remove\\n')
  write(channel, {versions: {}})
} else if (args.includes('add')) {
  const value = read(channel)
  const item = value.versions[${JSON.stringify(VERSION)}]
  if (flag('--expected-git-hash') !== item.gitHash) throw new Error('metadata hash mismatch')
  item.gitHash = flag('--git-hash')
  item.releaseNotes = JSON.parse(args.at(-2))
  item.security = args.includes('--security')
  write(channel, value)
} else throw new Error('unexpected start-cli args: ' + args.join(' '))
`,
  )
  nodeExecutable(
    'bin/gh',
    `
import { readFileSync, writeFileSync, existsSync, appendFileSync } from 'node:fs'
import { join } from 'node:path'
const root = process.env.FIXTURE_ROOT
const args = process.argv.slice(2)
if (args[0] === 'auth') process.exit(0)
const tag = args.find(arg => arg.startsWith('start-os/v'))
const path = join(root, tag.replace('/', '_') + '.json')
const flag = name => args[args.indexOf(name) + 1]
let data = existsSync(path) ? JSON.parse(readFileSync(path)) : null
if (args[1] === 'view') {
  if (!data) process.exit(1)
  console.log(args.includes('--jq') ? data.isDraft : JSON.stringify(data))
  process.exit(0)
}
appendFileSync(join(root, 'gh-events'), args.join(' ') + '\\n')
if (args[1] === 'create') data = {isDraft: true, assets: []}
if (!data) throw new Error('release missing')
if (args.includes('--notes-file')) data.body = readFileSync(flag('--notes-file'), 'utf8')
if (args.includes('--title')) data.title = flag('--title')
if (args.includes('--prerelease')) data.isPrerelease = true
if (args.includes('--draft=false')) data.isDraft = false
if (args[1] === 'upload') {
  if (process.env.FAIL_UPLOAD === '1') process.exit(42)
  const name = 'CHANGELOG.md'
  if (!data.assets.some(asset => asset.name === name)) data.assets.push({name})
}
writeFileSync(path, JSON.stringify(data))
`,
  )
  function release(success = true, extra = '') {
    const result = run(
      'bash',
      [
        '-c',
        `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"; PROJECT=start-os; KIND=os; VERSION=${VERSION}; TAG=${PREFIX}
OS_PLATFORMS=x86_64
cmd_pull() { mkdir -p "$(release_dir)"; }
cmd_push_gz() { :; }
cmd_sign() { :; }
generated_sections() { :; }
gpg() { :; }
b3sum() { :; }
s3cmd() { :; }
${extra}
cmd_pre_release
`,
      ],
      { encoding: 'utf8' },
    )
    if (success) assert.equal(result.status, 0, result.stdout + result.stderr)
    else assert.notEqual(result.status, 0)
    return result
  }
  const data = name => JSON.parse(readFileSync(join(root, name), 'utf8'))
  return { ...f, imageCommit, commit, alpha, release, data }
}

test('prerelease adopts the indexed image commit and allocates another RC after promotion failure', t => {
  const { env, write, git, root, imageCommit, commit, release, data } =
    prereleaseFixture(t)
  git('tag', 'start-os/v0.4.0-rc.99')
  write('projects/unrelated/file', 'later master change')
  const head = commit()
  assert.notEqual(head, imageCommit)
  env.FAIL_PROMOTE = '1'
  release(false)
  assert.equal(git('rev-parse', `${PREFIX}-rc.1^{commit}`).trim(), imageCommit)
  const published = data(`start-os_v${VERSION}-rc.1.json`)
  assert.equal(published.isPrerelease, true)
  assert.equal(published.isDraft, false)
  assert.equal(published.title, `StartOS v${VERSION}-rc.1`)
  assert.ok(published.body.includes('Security correction.'))
  assert.equal(data('alpha.json').versions[VERSION].gitHash, imageCommit)
  assert.equal(data('alpha.json').versions[VERSION].security, true)
  delete env.FAIL_PROMOTE
  release()
  assert.equal(
    git('tag', '--list', `${PREFIX}-rc.*`).trim(),
    `${PREFIX}-rc.1\n${PREFIX}-rc.2`,
  )
  assert.equal(data('beta.json').versions[VERSION].gitHash, imageCommit)
  assert.equal(data('beta.json').versions[VERSION].security, true)
  const notes = data('beta.json').versions[VERSION].releaseNotes
  assert.ok(notes.en_US.includes('Visible correction.'))
  assert.ok(notes.pl_PL.includes('Widoczna poprawka.'))
  assert.ok(notes.pl_PL.includes('Pełna lista zmian'))
  assert.equal(existsSync(join(root, 'removals')), false)
  assert.equal(
    JSON.parse(readFileSync(join(root, 'package.json'))).version,
    VERSION,
  )
})

test('another invocation creates a new RC even when the previous draft is incomplete', t => {
  const { env, release, git, data, root } = prereleaseFixture(t)
  env.FAIL_UPLOAD = '1'
  release(false)
  assert.equal(data(`start-os_v${VERSION}-rc.1.json`).isDraft, true)
  assert.equal(existsSync(join(root, 'promotions')), false)
  delete env.FAIL_UPLOAD
  release()
  assert.equal(data(`start-os_v${VERSION}-rc.1.json`).isDraft, true)
  assert.equal(data(`start-os_v${VERSION}-rc.2.json`).isDraft, false)
  assert.equal(
    git('tag', '--list', `${PREFIX}-rc.*`).trim(),
    `${PREFIX}-rc.1\n${PREFIX}-rc.2`,
  )
  assert.ok(
    readFileSync(join(root, 'gh-events'), 'utf8').includes('--latest=false'),
  )
})

test('the same image commit gets a new GitHub prerelease and replaces beta', t => {
  const { release, git, data, root, imageCommit } = prereleaseFixture(t)
  release()
  release()
  assert.equal(git('rev-parse', `${PREFIX}-rc.2^{commit}`).trim(), imageCommit)
  assert.equal(data(`start-os_v${VERSION}-rc.1.json`).isPrerelease, true)
  assert.equal(data(`start-os_v${VERSION}-rc.2.json`).isPrerelease, true)
  assert.deepEqual(Object.keys(data('beta.json').versions), [VERSION])
  assert.equal(data('beta.json').versions[VERSION].gitHash, imageCommit)
  assert.equal(readFileSync(join(root, 'removals'), 'utf8'), 'remove\n')
  assert.equal(
    readFileSync(join(root, 'promotions'), 'utf8'),
    'promote\npromote\n',
  )
})

test('new image commit increments the RC and replaces the old beta entry', t => {
  const { write, commit, alpha, release, git, data, root } =
    prereleaseFixture(t)
  release()
  write('projects/start-os/build-input', 'changed image')
  const next = commit()
  alpha(next)
  release()
  assert.equal(git('rev-parse', `${PREFIX}-rc.2^{commit}`).trim(), next)
  assert.equal(data('beta.json').versions[VERSION].gitHash, next)
  assert.equal(readFileSync(join(root, 'removals'), 'utf8'), 'remove\n')
  assert.ok(
    data(`start-os_v${VERSION}-rc.2.json`).body.includes(
      'Security correction.',
    ),
  )
  for (const number of [1, 2]) {
    assert.ok(
      existsSync(
        join(root, 'home/Downloads', `start-os_v${VERSION}-rc.${number}`),
      ),
    )
  }
})

test('missing image hash and explicit commit mismatch fail before tagging or publishing', t => {
  const { write, release, git, imageCommit, commit, env, root } =
    prereleaseFixture(t)
  write('projects/unrelated/file', 'later change')
  const other = commit()
  env.COMMIT = other
  const mismatch = release(false)
  assert.ok(
    mismatch.stderr.includes(
      'The tag has to point at the commit that produced the artifacts',
    ),
  )
  delete env.COMMIT
  write(
    'alpha.json',
    JSON.stringify({ versions: { [VERSION]: { sourceVersion: '>=0.3.5' } } }),
  )
  assert.ok(release(false).stderr.includes('has no gitHash'))
  assert.equal(git('tag', '--list', `${PREFIX}-rc.*`).trim(), '')
  assert.equal(existsSync(join(root, 'gh-events')), false)
  assert.notEqual(imageCommit, other)
})

test('docs sync excludes StartOS RC tags at the trigger', () => {
  const workflow = readFileSync(
    join(REPO, '.github/workflows/docs-sync-on-tag.yml'),
    'utf8',
  )
  assert.ok(workflow.includes("tags: ['*/v*', '!start-os/v*-rc.*']"))
})
