import assert from 'node:assert/strict'
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  unlinkSync,
} from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO, fixture } from './fixtures.mjs'

const VERSION = '1.2.3'
const MAIN =
  'Lede.\n\n## Highlights\n\n- Feature.\n\n## Important\n\nFollow up.\n'
const FRAMING = 'Lede.\n\n## Important\n\nFollow up.\n'
const PRE_UPDATE = '## ⚠️ Before You Update\n\n> Prepare first.'
const METADATA = '---\nuser-facing: true\n---\n\n'

function releaseFixture(t) {
  const f = fixture(t)
  const { root, env, write, run, git } = f
  mkdirSync(join(root, 'scripts'))
  for (const script of [
    'manage-release.sh',
    'changelog.mjs',
    'changelog-version.mjs',
  ])
    copyFileSync(join(REPO, 'scripts', script), join(root, 'scripts', script))
  for (const config of ['package.json', '.prettierrc.json'])
    copyFileSync(join(REPO, config), join(root, config))
  const labels = 'shared-libs/crates/start-core/locales/release-notes.json'
  write(labels, readFileSync(join(REPO, labels)))
  const main = write(`projects/start-sdk/release-notes/${VERSION}.md`, MAIN)
  const preUpdate = join(
    root,
    `projects/start-sdk/release-notes/${VERSION}.pre-update.md`,
  )
  Object.assign(env, { VERSION, CHANGELOG_REF: 'fixture-ref' })
  function script(
    command = 'notes',
    project = 'start-sdk',
    success = true,
    cwd = root,
  ) {
    const result = run(
      'bash',
      [join(cwd, 'scripts/manage-release.sh'), command, project],
      { cwd, encoding: 'utf8' },
    )
    if (success) assert.equal(result.status, 0, result.stderr)
    else assert.notEqual(result.status, 0)
    return result
  }
  function commit(message = 'fixture') {
    git('add', '.')
    git(
      '-c',
      'user.name=Fixture',
      '-c',
      'user.email=fixture@example.com',
      '-c',
      'commit.gpgsign=false',
      'commit',
      '-qm',
      message,
    )
  }
  function blockPython() {
    const marker = join(root, 'python-called')
    for (const name of ['python', 'python3']) {
      const path = write(
        `bin/${name}`,
        `#!/bin/sh\ntouch "${marker}"\nexit 97\n`,
      )
      chmodSync(path, 0o755)
    }
    env.PATH = `${join(root, 'bin')}:${env.PATH}`
    return marker
  }
  return { ...f, main, preUpdate, script, commit, blockPython }
}

const text = path => readFileSync(path, 'utf8')
const contains = (value, expected) =>
  assert.ok(
    value.includes(expected),
    `Expected ${JSON.stringify(expected)} in ${JSON.stringify(value)}`,
  )
const excludes = (value, unexpected) =>
  assert.ok(
    !value.includes(unexpected),
    `Unexpected ${JSON.stringify(unexpected)} in ${JSON.stringify(value)}`,
  )

const linkLast = notes =>
  assert.match(notes.trimEnd().split('\n').at(-1), /^\*\*\[.+\]\(.+\)\*\*/)

test('optional companion', t => {
  const { script, write } = releaseFixture(t)
  const without = script().stdout
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, '')
  assert.equal(script().stdout, without)
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  const combined = script().stdout
  assert.equal(combined, PRE_UPDATE + '\n\n' + without)
  linkLast(combined)
  assert.equal(combined.match(/Full changelog/g).length, 1)
  contains(combined, '/blob/fixture-ref/projects/start-sdk/CHANGELOG.md#123')
})

test('reads canonical version without override', t => {
  const { env, write, script } = releaseFixture(t)
  delete env.VERSION
  write('projects/start-sdk/package.json', '{"version":"1.2.3"}')
  contains(script().stdout, 'v1.2.3')
})

test('canonical versions and notes do not require Python', t => {
  const { env, write, script, blockPython } = releaseFixture(t)
  delete env.VERSION
  const marker = blockPython()
  for (const [project, manifest, contents, version, anchor] of [
    [
      'start-sdk',
      'projects/start-sdk/package.json',
      '{"version":"1.2.3"}',
      VERSION,
      '123',
    ],
    ['start-os', 'package.json', '{"version":"0.4.0.3"}', '0.4.0.3', '0403'],
    [
      'start-cli',
      'projects/start-cli/Cargo.toml',
      '[package]\nversion = "1.2.3"\n',
      VERSION,
      '123',
    ],
  ]) {
    write(manifest, contents)
    write(`projects/${project}/release-notes/${version}.md`, MAIN)
    const output = script('notes', project).stdout
    contains(output, '- Feature.')
    contains(output, `/projects/${project}/CHANGELOG.md#${anchor}`)
  }
  assert.equal(existsSync(marker), false, 'release caller invoked Python')
})

test('sparse deploy canonical versions and notes', t => {
  const { root, env, write, script, blockPython, git, commit, run } =
    releaseFixture(t)
  delete env.VERSION
  const marker = blockPython()
  write(
    'package.json',
    JSON.stringify({
      ...JSON.parse(text(join(REPO, 'package.json'))),
      version: '0.4.0.3',
    }),
  )
  write(
    'projects/start-wrt/backend/ctrl/Cargo.toml',
    '[package]\nname = "ctrl"\nversion = "1.2.3"\n',
  )
  const cases = [
    ['start-os', 'startos-iso.yaml', '0.4.0.3', '0403'],
    ['start-wrt', 'start-wrt.yaml', VERSION, '123'],
  ]
  for (const [project, , version] of cases) {
    write(`projects/${project}/release-notes/${version}.md`, FRAMING)
    write(
      `projects/${project}/changelog/patch-fixed-feature.md`,
      METADATA + '- Generated feature.\n',
    )
    write(
      `projects/${project}/release-notes/${version}.pre-update.md`,
      PRE_UPDATE,
    )
  }
  git('init', '-q')
  commit()
  const ref = git('rev-parse', 'HEAD').trim()
  env.CHANGELOG_REF = ref
  for (const [project, workflow, version, anchor] of cases) {
    const deploy = text(join(REPO, '.github/workflows', workflow)).split(
      '\n  deploy:\n',
    )[1]
    const sparse = deploy.match(
      /          sparse-checkout: \|\n((?:            .+\n)+)/,
    )
    assert.ok(sparse)
    const patterns =
      sparse[1]
        .trim()
        .split('\n')
        .map(line => line.trim())
        .join('\n') + '\n'
    const checkout = join(root, `checkout-${project}`)
    git('clone', '-q', '--no-checkout', root, checkout)
    const mode = deploy.includes('sparse-checkout-cone-mode: false')
      ? '--no-cone'
      : '--cone'
    git('-C', checkout, 'sparse-checkout', 'init', mode)
    const set = run(
      'git',
      ['-C', checkout, 'sparse-checkout', 'set', '--stdin'],
      { input: patterns, encoding: 'utf8' },
    )
    assert.equal(set.status, 0, set.stderr)
    git('-C', checkout, 'checkout', '-q')
    assert.equal(existsSync(join(checkout, '.prettierrc.json')), true)
    assert.equal(existsSync(join(checkout, 'node_modules')), false)
    assert.equal(existsSync(join(checkout, 'package.json')), true)
    const determine = deploy.match(
      /      - name: Determine version\n        id: version\n        run: \|\n((?:          .+\n)+)/,
    )
    assert.ok(determine)
    const outputFile = join(root, `outputs-${project}`)
    const result = run('bash', ['-e', '-c', determine[1]], {
      cwd: checkout,
      env: { ...env, GITHUB_OUTPUT: outputFile },
      encoding: 'utf8',
    })
    assert.equal(result.status, 0, result.stderr)
    assert.equal(text(outputFile), `version=${version}\n`)
    const derived = script('notes', project, true, checkout).stdout
    contains(derived, PRE_UPDATE)
    contains(derived, '- Generated feature.')
    contains(derived, 'Follow up.')
    contains(derived, `/tree/${ref}/projects/${project}/changelog`)
    const compose = deploy
      .split('      - name: Compile release notes\n')[1]
      ?.split('        run: |\n')[1]
      ?.split('\n      - ')[0]
    assert.ok(compose)
    assert.ok(
      deploy.indexOf('Compile release notes') <
        deploy.indexOf('Drop the previous alpha build'),
    )
    const composeShell = compose
      .split('\n')
      .map(line => line.slice(10))
      .join('\n')
      .replaceAll('${{ github.sha }}', ref)
    const runnerTemp = join(root, `notes-${project}`)
    mkdirSync(runnerTemp)
    const compile = () =>
      run('bash', ['-e', '-c', composeShell], {
        cwd: checkout,
        env: { ...env, VERSION: version, RUNNER_TEMP: runnerTemp },
        encoding: 'utf8',
      })
    const compiled = compile()
    assert.equal(compiled.status, 0, compiled.stderr)
    assert.deepEqual(JSON.parse(text(join(runnerTemp, 'release-notes.json'))), {
      releaseNotes: derived,
    })
    const fragmentPath = `checkout-${project}/projects/${project}/changelog/patch-fixed-feature.md`
    write(fragmentPath, '---\nuser-facing: invalid\n---\n\n- Invalid.\n')
    const invalid = compile()
    assert.notEqual(invalid.status, 0)
    contains(invalid.stderr, 'user-facing: true or false')
    write(fragmentPath, METADATA + '- Generated feature.\n')
    env.VERSION = version
    assert.equal(script('notes', project, true, checkout).stdout, derived)
    delete env.VERSION
  }
  assert.equal(
    existsSync(marker),
    false,
    'deploy release caller invoked Python',
  )
})

test('release links GitHub release without depending on tag CI', t => {
  const { env, write, script } = releaseFixture(t)
  delete env.CHANGELOG_REF
  for (const [project, version] of [
    ['start-sdk', '1.2.3'],
    ['start-os', '0.4.0.3'],
    ['start-os', '0.4.1'],
    ['start-sdk', '1.2.3-RC.1+Build.2'],
  ]) {
    env.VERSION = version
    write(`projects/${project}/release-notes/${version}.md`, MAIN)
    const output = script('notes', project).stdout
    contains(output, `/releases/tag/${project}/v${version}`)
    excludes(output, '/blob/')
    excludes(output, '/releases/download/')
  }
})

test('source links survive fragment consumption', t => {
  const { env, write, script, git, commit } = releaseFixture(t)
  git('init', '-q')
  write(`projects/start-sdk/release-notes/${VERSION}.md`, FRAMING)
  write(
    'projects/start-sdk/changelog/patch-fixed-example.md',
    METADATA + '- Fixed feature.\n',
  )
  write(
    'projects/start-sdk/CHANGELOG.md',
    '# Changelog\n\n## [1.2.2]\n\n- Previous release.\n',
  )
  commit()
  env.CHANGELOG_REF = 'HEAD'
  contains(script().stdout, '/tree/HEAD/projects/start-sdk/changelog')
  git('rm', 'projects/start-sdk/changelog/patch-fixed-example.md')
  write(
    'projects/start-sdk/CHANGELOG.md',
    '# Changelog\n\n## [1.2.3]\n\n- Fixed feature.\n',
  )
  commit()
  const output = script().stdout
  contains(output, '/blob/HEAD/projects/start-sdk/CHANGELOG.md#123')
  excludes(output, '/tree/')
})

test('GitHub body carries the notes and changelog without the link', t => {
  const { root, write, script, git, commit } = releaseFixture(t)
  git('init', '-q')
  write(`projects/start-sdk/release-notes/${VERSION}.md`, FRAMING)
  git('remote', 'add', 'origin', root)
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  write(
    'projects/start-sdk/changelog/patch-fixed-example.md',
    METADATA + '- Fixed feature.\n',
  )
  commit()
  const notes = script().stdout
  const body = script('body').stdout
  contains(notes, '**[Full changelog')
  excludes(body, '**[Full changelog')
  contains(
    body,
    `\n${PRE_UPDATE}\n\n${MAIN.replace('Feature.', 'Fixed feature.')}\n<details>`,
  )
  contains(body, '<details>\n<summary>Full changelog</summary>\n\n## [1.2.3]')
  contains(body, '- Fixed feature.\n\n</details>')
  excludes(body, '<details open')
  excludes(notes, '<details>')
  assert.equal(body.split(PRE_UPDATE).length - 1, 1)
})

test('registry commands reject invalid highlights before writes', t => {
  const { root, write, run } = releaseFixture(t)
  write(
    'projects/start-wrt/changelog/patch-fixed-invalid.md',
    '---\nuser-facing: invalid\n---\n\n- Invalid.\n',
  )
  const marker = join(root, 'registry-write')
  for (const command of [
    'refresh_registry_notes fixture-registry',
    'cmd_register',
  ]) {
    const result = run(
      'bash',
      [
        '-c',
        `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"; PROJECT=start-wrt; KIND=wrt; VERSION=1.2.3
load_registry_index() { _INDEX_JSON='{"versions":{"1.2.3":{"sourceVersion":">=0.1.0"}}}'; }
enter_release_dir() { :; }
start-cli() { touch "${marker}"; }
${command}
`,
      ],
      { encoding: 'utf8' },
    )
    assert.notEqual(result.status, 0)
    contains(result.stderr, 'user-facing: true or false')
    assert.equal(existsSync(marker), false)
  }
})

test('main framing is optional', t => {
  const { root, main, write, script, git, commit } = releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  commit()
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  unlinkSync(main)
  contains(script('notes').stdout, PRE_UPDATE)
  contains(script('body').stdout, '## Highlights')
})

test('existing changelog link is replaced', t => {
  const { write, script } = releaseFixture(t)
  write(
    `projects/start-sdk/release-notes/${VERSION}.md`,
    MAIN.replace('- Feature.', '- Feature.\n\n**[Full changelog old](old)\n'),
  )
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  const notes = script().stdout
  excludes(notes, '](old)')
  assert.equal(notes.match(/Full changelog/g).length, 1)
})

test('companion changes block adopted release', t => {
  const { root, preUpdate, write, run, git, commit } = releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  commit()
  let adopted = git('rev-parse', 'HEAD').trim()
  const command = `source <(awk '/^SUBCOMMAND=/{exit} {print}' "$1")
REPO_ROOT="$2"; PROJECT=start-sdk; VERSION=1.2.3; COMMIT="$3"
assert_metadata_matches_adopted`
  const check = () =>
    run(
      'bash',
      [
        '-c',
        command,
        'test',
        join(root, 'scripts/manage-release.sh'),
        root,
        adopted,
      ],
      { encoding: 'utf8' },
    )
  function blocked(name = '1.2.3.pre-update.md') {
    const result = check()
    assert.notEqual(result.status, 0)
    contains(result.stderr, name)
    contains(result.stderr, 'differs')
  }
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  blocked()
  git('add', preUpdate)
  blocked()
  write(
    `projects/start-sdk/release-notes/${VERSION}.md`,
    MAIN + 'Uncommitted instruction.\n',
  )
  blocked('1.2.3.md')
  write(`projects/start-sdk/release-notes/${VERSION}.md`, MAIN)
  commit()
  blocked()
  adopted = git('rev-parse', 'HEAD').trim()
  write(
    `projects/start-sdk/release-notes/${VERSION}.pre-update.md`,
    PRE_UPDATE + '\nChanged warning.\n',
  )
  commit()
  blocked()
  adopted = git('rev-parse', 'HEAD').trim()
  unlinkSync(preUpdate)
  commit()
  blocked()
  adopted = git('rev-parse', 'HEAD').trim()
  write('projects/start-sdk/CHANGELOG.md', '')
  commit()
  const result = check()
  assert.equal(result.status, 0, result.stderr)
  write(
    'projects/start-sdk/changelog/patch-fixed-example.md',
    '- Fixed a released feature.\n',
  )
  commit()
  blocked('changelog')
})

test('adopted release accepts compiled metadata after fragments are consumed', t => {
  const { root, write, run, git, commit } = releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  write(`projects/start-sdk/release-notes/${VERSION}.md`, 'Lede.\n')
  const path = write(
    'projects/start-sdk/changelog/patch-fixed-public.md',
    METADATA + '- Released fix.\n',
  )
  const translation = write(
    'projects/start-sdk/changelog/patch-fixed-public.pl_PL.md',
    '- Poprawka.\n',
  )
  const localizedPath = `projects/start-sdk/release-notes/${VERSION}.pl_PL.md`
  write(localizedPath, '## Nowości\n\n<!-- highlights -->\n')
  commit()
  const adopted = git('rev-parse', 'HEAD').trim()
  git('-c', 'tag.gpgsign=false', 'tag', 'start-sdk/v1.2.3')
  const synced = run(process.execPath, ['scripts/changelog.mjs', 'sync'])
  assert.equal(synced.status, 0, synced.stderr.toString())
  assert.equal(existsSync(path), false)
  assert.equal(existsSync(translation), false)
  commit()
  const command = `source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"; PROJECT=start-sdk; VERSION=1.2.3; COMMIT="${adopted}"
assert_metadata_matches_adopted`
  const check = () => run('bash', ['-c', command], { encoding: 'utf8' })
  const result = check()
  assert.equal(result.status, 0, result.stderr)
  const localized = text(join(root, localizedPath))
  write(localizedPath, localized + '\nChanged translated notice.\n')
  const translatedChange = check()
  assert.notEqual(translatedChange.status, 0)
  contains(translatedChange.stderr, 'release-note bundle')
  write(localizedPath, localized)
  write(
    'projects/start-sdk/changelog/patch-fixed-other.md',
    METADATA + '- New change.\n',
  )
  assert.notEqual(check().status, 0)
})

test('release uploads changelog from tag before publication', t => {
  const { root, env, write, script, git, commit, nodeExecutable, run } =
    releaseFixture(t)
  delete env.CHANGELOG_REF
  git('init', '-q')
  write(`projects/start-sdk/release-notes/${VERSION}.md`, FRAMING)
  write(
    'projects/start-sdk/CHANGELOG.md',
    '# Changelog\n\n## [1.2.2]\n\n- Old change.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-fixed-example.md',
    METADATA + '- Fixed the released feature.\n',
  )
  commit()
  git('-c', 'tag.gpgsign=false', 'tag', 'start-sdk/v1.2.3')
  const remote = join(root, 'origin.git')
  git('init', '--bare', '-q', remote)
  git('remote', 'add', 'origin', remote)
  git('push', '-q', 'origin', 'HEAD:master', '--tags')
  write(
    'projects/start-sdk/changelog/minor-added-later.md',
    METADATA + '- Later change.\n',
  )
  git('add', 'projects')
  git(
    '-c',
    'user.name=Fixture',
    '-c',
    'user.email=fixture@example.com',
    '-c',
    'commit.gpgsign=false',
    'commit',
    '-qm',
    'later',
  )
  write(`projects/start-sdk/release-notes/${VERSION}.md`, 'Later lede.\n')
  write(
    `projects/start-sdk/release-notes/${VERSION}.pre-update.md`,
    PRE_UPDATE + '\nLater warning.\n',
  )
  const captured = join(root, 'uploaded.md')
  const bodyFile = join(root, 'release-body.md')
  const calls = join(root, 'gh-calls')
  nodeExecutable(
    'bin/gh',
    `import { appendFileSync, copyFileSync } from 'node:fs'
const args = process.argv.slice(2)
appendFileSync(process.env.CALLS, JSON.stringify(args) + '\\n')
if (args[1] === 'view') {
  if (!process.env.EXISTING_RELEASE) process.exit(1)
  console.log(process.env.RELEASE_DRAFT || 'false')
}
if (args.includes('--notes-file'))
  copyFileSync(args[args.indexOf('--notes-file') + 1], process.env.BODY)
if (args[1] === 'upload') {
  if (process.env.FAIL_UPLOAD) process.exit(1)
  copyFileSync(args.find(arg => arg.endsWith('/CHANGELOG.md')), process.env.CAPTURE)
}
if (args.includes('--draft=false') && process.env.FAIL_PUBLISH) process.exit(1)
`,
  )
  Object.assign(env, {
    PATH: `${join(root, 'bin')}:${env.PATH}`,
    CAPTURE: captured,
    BODY: bodyFile,
    CALLS: calls,
  })
  const ghCalls = () =>
    text(calls)
      .trim()
      .split('\n')
      .map(line => JSON.parse(line))
  const taggedCommit = git('rev-parse', 'start-sdk/v1.2.3')
  script('create-gh-release')
  let recorded = ghCalls()
  assert.deepEqual(
    recorded.map(args => args[1]),
    ['view', 'create', 'upload', 'edit'],
  )
  assert.ok(recorded[1].includes('--draft'))
  assert.ok(recorded[1].includes('--verify-tag'))
  assert.ok(recorded[3].includes('--draft=false'))
  const body = text(bodyFile)
  excludes(body, '**[Full changelog')
  contains(body, '<details>\n<summary>Full changelog</summary>\n\n## [1.2.3]')
  contains(body, 'Fixed the released feature.\n\n</details>')
  excludes(body, 'Old change.')
  excludes(body, 'Later change.')
  excludes(body, 'Later lede.')
  excludes(body, 'Later warning.')
  contains(body, 'Lede.')
  excludes(body, '<details open')
  const uploaded = text(captured)
  contains(uploaded, '## [1.2.3]')
  contains(uploaded, 'Fixed the released feature.')
  contains(uploaded, 'Old change.')
  excludes(uploaded, 'Later change.')
  assert.equal(git('rev-parse', 'start-sdk/v1.2.3'), taggedCommit)

  env.EXISTING_RELEASE = '1'
  for (const draft of ['true', 'false']) {
    env.RELEASE_DRAFT = draft
    unlinkSync(calls)
    script('create-gh-release')
    recorded = ghCalls()
    assert.deepEqual(
      recorded.map(args => args[1]),
      draft === 'true'
        ? ['view', 'edit', 'upload', 'edit']
        : ['view', 'edit', 'upload'],
    )
    assert.equal(text(bodyFile), body)
    assert.equal(text(captured), uploaded)
    assert.ok(!recorded[1].includes('--draft'))
  }

  env.FAIL_UPLOAD = '1'
  env.EXISTING_RELEASE = ''
  unlinkSync(calls)
  const published = join(root, 'published')
  const failed = run(
    'bash',
    [
      '-c',
      `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"
PROJECT=start-sdk; KIND=npm; VERSION=1.2.3; TAG=start-sdk/v1.2.3
cmd_pre_check() { :; }
cmd_tag() { :; }
cmd_push() { touch "${published}"; }
cmd_release
`,
    ],
    { encoding: 'utf8' },
  )
  assert.notEqual(failed.status, 0)
  assert.equal(existsSync(published), false)
  assert.ok(!ghCalls().some(args => args.includes('--draft=false')))
  delete env.FAIL_UPLOAD

  env.EXISTING_RELEASE = '1'
  env.RELEASE_DRAFT = 'true'
  env.FAIL_PUBLISH = '1'
  script('create-gh-release', 'start-sdk', false)
  delete env.FAIL_PUBLISH
  script('create-gh-release')
  write(
    'projects/start-sdk/changelog/patch-fixed-example.md',
    'invalid fragment\n',
  )
  git('add', 'projects')
  git(
    '-c',
    'user.name=Fixture',
    '-c',
    'user.email=fixture@example.com',
    '-c',
    'commit.gpgsign=false',
    'commit',
    '-qm',
    'invalid',
  )
  git('-c', 'tag.gpgsign=false', 'tag', '-f', 'start-sdk/v1.2.3')
  git('push', '-q', '--force', 'origin', 'refs/tags/start-sdk/v1.2.3')
  unlinkSync(captured)
  for (const existing of ['', '1']) {
    env.EXISTING_RELEASE = existing
    if (existsSync(calls)) unlinkSync(calls)
    script('create-gh-release', 'start-sdk', false)
    assert.equal(existsSync(captured), false)
    assert.equal(existsSync(calls), false)
  }
})

test('resume requires a published release with inline changelog and attachment', t => {
  const { root, env, run, git, commit, nodeExecutable } = releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  commit()
  nodeExecutable(
    'bin/gh',
    `
if (!process.env.RELEASE_METADATA) process.exit(1)
console.log(process.env.RELEASE_METADATA)
`,
  )
  env.PATH = `${join(root, 'bin')}:${env.PATH}`
  const command = `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"
PROJECT=start-wrt; KIND=wrt; VERSION=1.2.3; TAG=start-wrt/v1.2.3
missing_registry_assets() { echo missing; }
detect_progress
printf '%s' "\${DONE[create-gh-release]:-incomplete}"
`
  for (const [metadata, complete] of [
    [null, false],
    [{ isDraft: false, assets: [], body: '' }, false],
    [{ isDraft: false, assets: [{ name: 'CHANGELOG.md' }], body: MAIN }, false],
    [
      { isDraft: false, assets: [], body: '<summary>Full changelog</summary>' },
      false,
    ],
    [
      {
        isDraft: true,
        assets: [{ name: 'CHANGELOG.md' }],
        body: '<summary>Full changelog</summary>',
      },
      false,
    ],
    [
      {
        isDraft: false,
        assets: [{ name: 'CHANGELOG.md' }],
        body: '<summary>Full changelog</summary>',
      },
      true,
    ],
  ]) {
    env.RELEASE_METADATA = metadata ? JSON.stringify(metadata) : ''
    const result = run('bash', ['-c', command], { encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    assert.equal(result.stdout, complete ? '1' : 'incomplete')
  }
})

test('release without fragments still has an expandable changelog', t => {
  const { root, script, git, commit } = releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  commit()
  const body = script('body').stdout
  contains(
    body,
    '<summary>Full changelog</summary>\n\nNo changelog entries.\n\n</details>',
  )
})

test('StartOS registry and packaged welcome', t => {
  const { root, env, write, script, run, git, commit } = releaseFixture(t)
  const version = '0.4.0.2'
  write('projects/start-os/build/lib/firmware.json', '[]')
  write('projects/start-os/build/dpkg-deps/fixture', '')
  write('build/bash-completion', '')
  const source = join(REPO, 'projects/start-os/release-notes')
  for (const name of [`${version}.md`, `${version}.pre-update.md`])
    write(
      `projects/start-os/release-notes/${name}`,
      readFileSync(join(source, name)),
    )
  write(
    `projects/start-os/release-notes/${version}.pl_PL.md`,
    'Polskie nowości\n',
  )
  write(`projects/start-os/release-notes/${version}.de_DE.md`, 'Neuigkeiten\n')
  write('projects/start-os/release-notes/0.4.0.1.pl_PL.md', 'Starsze wydanie\n')
  env.VERSION = version
  const combined = script('notes', 'start-os').stdout
  contains(combined, '## ⚠️ Before You Update')
  contains(combined, 'only way to update from 0.3.5.1')
  contains(combined, text(join(source, `${version}.pre-update.md`)).trim())
  const stage = join(root, 'image/usr/lib/startos')
  mkdirSync(stage, { recursive: true })
  const versionFile = write('VERSION.txt', version)
  const renderTemps = () =>
    readdirSync(root).filter(name => name.startsWith('.release-notes.'))
  const copies = join(root, 'copies')
  const harness = write(
    'install.mk',
    `mkdir = $(if $(filter %/release-notes,$(1)),mkdir -p "$(1)",true)
rm = true
ln = true
ls-files = $(wildcard $(1)/*)
cp = $(if $(filter %/release-notes.md,$(2))$(findstring /release-notes/,$(2)),printf '%s\\n' "$(2)" >> "${copies}"; cp "$(1)" "$(2)",true)
include ${REPO}/projects/start-os/build.mk
notes-inputs:
\t@printf '%s\\n' $(filter $(STARTOS_RELEASE_NOTES_SRC),$(STARTOS_TARGETS))
`,
  )
  const inputs = run(
    'make',
    ['-f', harness, 'notes-inputs', `VERSION_FILE=${versionFile}`],
    { cwd: root, encoding: 'utf8' },
  )
  assert.equal(inputs.status, 0, inputs.stderr)
  contains(inputs.stdout, 'scripts/changelog.mjs')
  contains(
    inputs.stdout,
    'shared-libs/crates/start-core/locales/release-notes.json',
  )
  contains(inputs.stdout, `${version}.md`)
  contains(inputs.stdout, `${version}.pl_PL.md`)
  contains(inputs.stdout, `${version}.de_DE.md`)
  excludes(inputs.stdout, 'pre-update')
  excludes(inputs.stdout, '0.4.0.1')
  const install = (makefile = harness) =>
    run(
      'make',
      [
        '-f',
        makefile,
        'start-os-install',
        'STARTOS_TARGETS=',
        'PLATFORM=x86_64',
        `VERSION_FILE=${versionFile}`,
        `DESTDIR=${root}/image`,
        'ENVIRONMENT=',
      ],
      { cwd: root, encoding: 'utf8' },
    )
  const result = install()
  assert.equal(result.status, 0, result.stderr)
  const packaged = text(join(stage, 'release-notes.md'))
  assert.equal(packaged, text(join(source, `${version}.md`)))
  excludes(packaged, 'Before You Update')
  excludes(packaged, 'only way to update')
  contains(packaged, '## Highlights')
  assert.equal(text(join(stage, 'release-notes/pl_PL.md')), 'Polskie nowości\n')
  assert.equal(text(join(stage, 'release-notes/de_DE.md')), 'Neuigkeiten\n')
  assert.deepEqual(readdirSync(join(stage, 'release-notes')).sort(), [
    'de_DE.md',
    'pl_PL.md',
  ])
  assert.deepEqual(text(copies).trim().split('\n'), [
    join(stage, 'release-notes.md'),
    join(stage, 'release-notes/de_DE.md'),
    join(stage, 'release-notes/pl_PL.md'),
  ])
  assert.deepEqual(renderTemps(), [])

  git('init', '-q')
  write(
    `projects/start-os/release-notes/${version}.md`,
    'Lede.\n\n## Important\n\nKeep this notice.\n',
  )
  write(
    'projects/start-os/changelog/patch-fixed-public.md',
    METADATA + '- Generated highlight.\n',
  )
  write(
    'projects/start-os/changelog/patch-changed-internal.md',
    '---\nuser-facing: false\n---\n\n- Internal detail.\n',
  )
  write(
    `projects/start-os/release-notes/${version}.pl_PL.md`,
    'Polska wersja.\n\n## Nowości\n\n<!-- highlights -->\n',
  )
  write(
    `projects/start-os/release-notes/${version}.de_DE.md`,
    '## Neuigkeiten\n\n<!-- highlights -->\n',
  )
  write(
    'projects/start-os/changelog/patch-fixed-public.pl_PL.md',
    '- Wygenerowana nowość.\n',
  )
  write(
    'projects/start-os/changelog/patch-fixed-public.fr_FR.md',
    '- Nouveauté générée.\n',
  )
  const polish = 'Polska wersja.\n\n## Nowości\n\n- Wygenerowana nowość.\n'
  const german = '## Neuigkeiten\n\n- Generated highlight.\n'
  const french = '## Nouveautés\n\n- Nouveauté générée.\n'
  commit()
  const generated = install()
  assert.equal(generated.status, 0, generated.stderr)
  const welcome = text(join(stage, 'release-notes.md'))
  assert.equal(
    welcome,
    'Lede.\n\n## Highlights\n\n- Generated highlight.\n\n## Important\n\nKeep this notice.\n',
  )
  const registry = script('notes', 'start-os').stdout
  contains(registry, 'Generated highlight.')
  contains(registry, PRE_UPDATE.split('\n')[0])
  excludes(welcome, 'Before You Update')
  excludes(registry, 'Internal detail.')
  assert.equal(text(join(stage, 'release-notes/pl_PL.md')), polish)
  assert.equal(text(join(stage, 'release-notes/de_DE.md')), german)
  assert.equal(
    text(join(stage, 'release-notes/fr_FR.md')),
    'Lede.\n\n## Nouveautés\n\n- Nouveauté générée.\n\n## Important\n\nKeep this notice.\n',
  )
  assert.deepEqual(renderTemps(), [])

  unlinkSync(join(root, `projects/start-os/release-notes/${version}.md`))
  const withoutFraming = install()
  assert.equal(withoutFraming.status, 0, withoutFraming.stderr)
  const generatedOnly = text(join(stage, 'release-notes.md'))
  assert.equal(generatedOnly, '## Highlights\n\n- Generated highlight.\n')

  const remoteCopy = readFileSync(join(REPO, 'build/common.mk'), 'utf8')
    .match(/define cp\n[\s\S]+?\nendef/)[0]
    .replace('define cp', 'define remote-copy')
  const remoteHarness = write(
    'remote-install.mk',
    `${text(harness)}
TAR_BIN = tar
ssh = sh -c $(subst sudo ,,$1)
${remoteCopy}
cp = $(if $(filter %/release-notes.md,$(2))$(findstring /release-notes/,$(2)),$(call remote-copy,$(1),$(2)),true)
`,
  )
  for (const path of [
    'release-notes.md',
    'release-notes/de_DE.md',
    'release-notes/fr_FR.md',
    'release-notes/pl_PL.md',
  ])
    unlinkSync(join(stage, path))
  const remote = install(remoteHarness)
  assert.equal(remote.status, 0, remote.stderr)
  assert.equal(text(join(stage, 'release-notes.md')), generatedOnly)
  assert.equal(text(join(stage, 'release-notes/de_DE.md')), german)
  assert.equal(text(join(stage, 'release-notes/pl_PL.md')), polish)
  assert.equal(text(join(stage, 'release-notes/fr_FR.md')), french)
  assert.deepEqual(renderTemps(), [])

  const copiedBeforeFailure = text(copies)
  write(
    'projects/start-os/changelog/patch-fixed-public.md',
    '---\nuser-facing: invalid\n---\n\n- Invalid.\n',
  )
  const failed = install()
  assert.notEqual(failed.status, 0)
  contains(failed.stderr, 'user-facing: true or false')
  assert.equal(text(join(stage, 'release-notes.md')), generatedOnly)
  assert.equal(text(copies), copiedBeforeFailure)
  assert.deepEqual(renderTemps(), [])

  write(
    'projects/start-os/changelog/patch-fixed-public.md',
    METADATA + '- Updated English highlight.\n',
  )
  write(
    `projects/start-os/release-notes/${version}.pl_PL.md`,
    '## Nowości\n\n- Ręcznie napisane.\n',
  )
  const localeFailure = install()
  assert.notEqual(localeFailure.status, 0)
  contains(localeFailure.stderr, 'localized framing needs <!-- highlights -->')
  assert.equal(text(join(stage, 'release-notes.md')), generatedOnly)
  assert.equal(text(copies), copiedBeforeFailure)
  assert.deepEqual(renderTemps(), [])
})

test('registry payload composes localized notes, warnings, fallbacks and links', t => {
  const f = releaseFixture(t)
  const { root, env, write, script, run, git, commit } = f
  delete env.CHANGELOG_REF
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  write(`projects/start-sdk/release-notes/${VERSION}.md`, FRAMING)
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  write(
    `projects/start-sdk/release-notes/${VERSION}.pl_PL.md`,
    'Wstęp.\n\n## Nowości\n\n<!-- highlights -->\n\n## Ważne\n\nWażna uwaga.\n',
  )
  write(
    `projects/start-sdk/release-notes/${VERSION}.pl_PL.pre-update.md`,
    '## Przed aktualizacją\n\nPolskie ostrzeżenie.\n',
  )
  write(
    `projects/start-sdk/release-notes/${VERSION}.de_DE.md`,
    '## Neuigkeiten\n\n<!-- highlights -->\n\n## Wichtig\n\nHinweis.\n',
  )
  write(`projects/start-sdk/release-notes/${VERSION}.de_DE.pre-update.md`, '')
  write(
    `projects/start-sdk/release-notes/${VERSION}.es_ES.md`,
    '## Novedades\n\n<!-- highlights -->\n',
  )
  write(
    `projects/start-sdk/release-notes/${VERSION}.fr_FR.pre-update.md`,
    '## Avant la mise à jour\n\nAttention.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-added-feature.md',
    METADATA + '- Feature.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-added-feature.pl_PL.md',
    '- Funkcja.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-fixed-other.md',
    METADATA + '- Another fix.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-changed-internal.md',
    '---\nuser-facing: false\n---\n\n- Internal.\n',
  )
  commit()
  const snapshot = git('rev-parse', 'HEAD').trim()
  const payload = () => JSON.parse(script('registry-notes').stdout)
  const notes = payload()
  const translations = notes.releaseNotes
  const english = translations.en_US
  assert.deepEqual(Object.keys(translations).sort(), [
    'de_DE',
    'en_US',
    'es_ES',
    'fr_FR',
    'pl_PL',
  ])
  assert.equal(script('notes').stdout, english)
  contains(english, PRE_UPDATE)
  contains(english, 'Lede.')
  contains(english, 'Follow up.')
  contains(english, '/releases/tag/start-sdk/v1.2.3')
  linkLast(english)
  contains(translations.pl_PL, 'Wstęp.')
  contains(translations.pl_PL, 'Ważna uwaga.')
  contains(translations.pl_PL, '- Funkcja.')
  contains(translations.pl_PL, '- Another fix.')
  contains(translations.pl_PL, 'Polskie ostrzeżenie.')
  contains(translations.pl_PL, 'Pełna lista zmian')
  excludes(translations.pl_PL, 'Prepare first.')
  linkLast(translations.pl_PL)
  contains(translations.de_DE, PRE_UPDATE)
  contains(translations.de_DE, '- Feature.')
  contains(translations.de_DE, '- Another fix.')
  contains(translations.de_DE, 'Vollständiges Änderungsprotokoll')
  linkLast(translations.de_DE)
  contains(translations.es_ES, PRE_UPDATE)
  contains(translations.es_ES, 'Registro completo de cambios')
  contains(translations.fr_FR, 'Attention.')
  contains(translations.fr_FR, '- Feature.')
  contains(translations.fr_FR, 'Journal complet des modifications')
  for (const body of [english, ...Object.values(translations)]) {
    excludes(body, 'Internal.')
    excludes(body, '<!-- highlights -->')
  }
  const welcome = run(
    process.execPath,
    ['scripts/changelog.mjs', 'notes-bundle', 'projects/start-sdk', VERSION],
    { encoding: 'utf8' },
  )
  assert.equal(welcome.status, 0, welcome.stderr)
  excludes(welcome.stdout, 'Prepare first.')
  excludes(welcome.stdout, 'Polskie ostrzeżenie.')
  excludes(welcome.stdout, 'Attention.')
  excludes(welcome.stdout, 'Full changelog')
  const github = script('body').stdout
  contains(github, PRE_UPDATE)
  contains(github, 'Lede.')
  contains(github, 'Follow up.')
  excludes(github, 'Wstęp.')
  excludes(github, 'Full changelog for')

  env.CHANGELOG_REF = snapshot
  const pending = payload()
  contains(pending.releaseNotes.en_US, 'Full changelog fragments for v1.2.3')
  contains(pending.releaseNotes.en_US, 'changes awaiting release.')
  for (const [locale, label, description] of [
    [
      'de_DE',
      'Alle Änderungsfragmente für v1.2.3',
      'noch nicht veröffentlichte Änderungen.',
    ],
    [
      'es_ES',
      'Fragmentos del registro completo de cambios de v1.2.3',
      'cambios pendientes de publicación.',
    ],
    [
      'fr_FR',
      'Fragments du journal complet des modifications de la v1.2.3',
      'modifications en attente de publication.',
    ],
    [
      'pl_PL',
      'Fragmenty pełnej listy zmian w wersji 1.2.3',
      'zmiany oczekujące na wydanie.',
    ],
  ]) {
    const body = pending.releaseNotes[locale]
    contains(body, label)
    contains(body, description)
    contains(body, `/tree/${snapshot}/projects/start-sdk/changelog`)
  }
  env.NOTES_REF = snapshot
  write(
    `projects/start-sdk/release-notes/${VERSION}.pl_PL.pre-update.md`,
    'Later warning.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-fixed-other.md',
    METADATA + '- Later fix.\n',
  )
  assert.deepEqual(
    payload(),
    pending,
    'publishing from a ref uses its notes and warnings',
  )
  delete env.NOTES_REF
  delete env.CHANGELOG_REF
})

test('registry and GitHub notes include localized Security without summary files', t => {
  const { root, main, write, script, git, commit } = releaseFixture(t)
  unlinkSync(main)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  write(
    'projects/start-sdk/changelog/patch-added-feature.md',
    METADATA + '- Feature.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-security-fix.md',
    METADATA + '- Security fix.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-security-fix.pl_PL.md',
    '- Poprawka bezpieczeństwa.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-security-internal.md',
    '---\nuser-facing: false\n---\n\n- Internal security detail.\n',
  )
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  commit()
  const notes = JSON.parse(script('registry-notes').stdout).releaseNotes
  for (const [locale, highlights, security, body] of [
    ['en_US', 'Highlights', 'Security', 'Security fix.'],
    ['pl_PL', 'Nowości', 'Bezpieczeństwo', 'Poprawka bezpieczeństwa.'],
  ]) {
    contains(notes[locale], `## ${highlights}`)
    contains(notes[locale], `## ${security}\n\n- ${body}`)
    contains(notes[locale], PRE_UPDATE)
    excludes(notes[locale], 'Internal security detail.')
    linkLast(notes[locale])
  }
  assert.ok(
    !existsSync(join(root, `projects/start-sdk/release-notes/${VERSION}.md`)),
  )
  assert.ok(
    !existsSync(
      join(root, `projects/start-sdk/release-notes/${VERSION}.pl_PL.md`),
    ),
  )
  const github = script('body').stdout
  contains(github, '## Security\n\n- Security fix.')
  excludes(github, 'Poprawka bezpieczeństwa.')
  write(
    `projects/start-sdk/release-notes/${VERSION}.md`,
    '## Important\n\nKeep this notice.\n',
  )
  const fallback = JSON.parse(script('registry-notes').stdout).releaseNotes
    .pl_PL
  contains(fallback, '## Important\n\nKeep this notice.')
  assert.ok(
    fallback.indexOf('## Bezpieczeństwo') < fallback.indexOf('## Important'),
  )
})

test('older source catalogs fall back to shared pending-fragment labels', t => {
  const { root, env, write, script, git, commit } = releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  const catalog = 'shared-libs/crates/start-core/locales/release-notes.json'
  const current = JSON.parse(text(join(root, catalog)))
  const older = structuredClone(current)
  delete older['release-notes.full-changelog-fragments']
  write(catalog, JSON.stringify(older))
  write(`projects/start-sdk/release-notes/${VERSION}.md`, FRAMING)
  write(
    `projects/start-sdk/release-notes/${VERSION}.pl_PL.md`,
    '## Nowości\n\n<!-- highlights -->\n',
  )
  write(
    'projects/start-sdk/changelog/patch-fixed-feature.md',
    METADATA + '- Feature.\n',
  )
  commit()
  const snapshot = git('rev-parse', 'HEAD').trim()
  write(catalog, JSON.stringify(current))
  env.NOTES_REF = snapshot
  env.CHANGELOG_REF = snapshot
  const notes = JSON.parse(script('registry-notes').stdout)
  contains(notes.releaseNotes.en_US, 'Full changelog fragments for v1.2.3')
  contains(
    notes.releaseNotes.pl_PL,
    'Fragmenty pełnej listy zmian w wersji 1.2.3',
  )
  contains(notes.releaseNotes.pl_PL, 'zmiany oczekujące na wydanie.')
  contains(
    notes.releaseNotes.pl_PL,
    `/tree/${snapshot}/projects/start-sdk/changelog`,
  )
})

test('registry publishers pass the complete LocaleString as the release-notes argument', t => {
  const { root, env, write, run, script } = releaseFixture(t)
  write(`projects/start-wrt/release-notes/${VERSION}.md`, MAIN)
  write(
    `projects/start-wrt/release-notes/${VERSION}.pl_PL.md`,
    'Polskie nowości.\n',
  )
  const expected = JSON.parse(script('registry-notes', 'start-wrt').stdout)
  const capture = join(root, 'arguments')
  const harness = `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"; PROJECT=start-wrt; KIND=wrt; VERSION=1.2.3
load_registry_index() { _INDEX_JSON='{"versions":{"1.2.3":{"sourceVersion":">=0.1.0"}}}'; }
enter_release_dir() { :; }
release_files() { :; }
start-cli() {
    if [ "$1" = registry ]; then echo '<RELEASE_NOTES_OR_JSON>'
    else printf '%s\\0' "$@" > "${capture}"
    fi
}
`
  for (const command of [
    'refresh_registry_notes fixture-registry',
    'cmd_register',
  ]) {
    const result = run('bash', ['-c', harness + command], { encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    const args = text(capture).split('\0').slice(0, -1)
    assert.deepEqual(args.slice(1, 7), [
      'registry',
      'os',
      'version',
      'add',
      VERSION,
      `v${VERSION}`,
    ])
    assert.deepEqual(JSON.parse(args[7]), expected.releaseNotes)
    assert.equal(args.length, 9)
  }
  for (const [workflow, name] of [
    ['startos-iso.yaml', 'Register OS version'],
    ['start-wrt.yaml', 'Register StartWRT version in the target registry'],
  ]) {
    const source = text(join(REPO, '.github/workflows', workflow))
    const step = source
      .split(`      - name: ${name}\n`)[1]
      .split('        run: |\n')[1]
      .split('\n      - ')[0]
    const shell = step
      .split('\n')
      .map(line => line.slice(10))
      .join('\n')
      .replaceAll('${{ env.REGISTRY }}', 'fixture-registry')
    const runnerTemp = join(root, 'runner')
    write('runner/release-notes.json', JSON.stringify(expected))
    const result = run(
      'bash',
      [
        '-e',
        '-c',
        `start-cli() { printf '%s\\0' "$@" > "${capture}"; }\n${shell}`,
      ],
      { encoding: 'utf8', env: { ...env, RUNNER_TEMP: runnerTemp } },
    )
    assert.equal(result.status, 0, result.stderr)
    const args = text(capture).split('\0').slice(0, -1)
    assert.deepEqual(JSON.parse(args[7]), expected.releaseNotes)
    assert.equal(args.length, 9)
  }
})

test('localized compiler failures prevent registry writes and warning edits block alpha adoption', t => {
  const { root, write, run, git, commit } = releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  write(`projects/start-wrt/release-notes/${VERSION}.md`, FRAMING)
  write(
    `projects/start-wrt/release-notes/${VERSION}.pl_PL.md`,
    '## Nowości\n\n<!-- highlights -->\n',
  )
  write(
    'projects/start-wrt/changelog/patch-fixed-feature.md',
    METADATA + '- Feature.\n',
  )
  commit()
  const adopted = git('rev-parse', 'HEAD').trim()
  const harness = `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"; PROJECT=start-wrt; KIND=wrt; VERSION=1.2.3; COMMIT="${adopted}"
`
  const check = () =>
    run('bash', ['-c', harness + 'assert_metadata_matches_adopted'], {
      encoding: 'utf8',
    })
  assert.equal(check().status, 0)
  const warning = `projects/start-wrt/release-notes/${VERSION}.pl_PL.pre-update.md`
  write(warning, 'Translated preparation.\n')
  const changed = check()
  assert.notEqual(changed.status, 0)
  contains(changed.stderr, 'localized pre-update warnings')
  unlinkSync(join(root, warning))
  const marker = join(root, 'registry-write')
  write(
    `projects/start-wrt/release-notes/${VERSION}.pl_PL.md`,
    '## Nowości\n\n- Invalid authored highlights.\n',
  )
  for (const command of [
    'refresh_registry_notes fixture-registry',
    'cmd_register',
  ]) {
    const failed = run(
      'bash',
      [
        '-c',
        harness +
          `
load_registry_index() { touch "${marker}"; }
enter_release_dir() { touch "${marker}"; }
start-cli() { touch "${marker}"; }
${command}
`,
      ],
      { encoding: 'utf8' },
    )
    assert.notEqual(failed.status, 0)
    contains(failed.stderr, 'localized framing needs <!-- highlights -->')
    assert.equal(existsSync(marker), false)
  }
})

test('registry source refs pin shared link translations and reject mismatched alpha metadata', t => {
  const { root, env, write, script, run, git, commit } = releaseFixture(t)
  delete env.CHANGELOG_REF
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  write(`projects/start-sdk/release-notes/${VERSION}.md`, FRAMING)
  write(
    `projects/start-sdk/release-notes/${VERSION}.pl_PL.md`,
    '## Nowości\n\n<!-- highlights -->\n',
  )
  write(
    'projects/start-sdk/changelog/patch-fixed-feature.md',
    METADATA + '- Feature.\n',
  )
  commit()
  const adopted = git('rev-parse', 'HEAD').trim()
  const original = JSON.parse(script('registry-notes').stdout)
  const catalog = 'shared-libs/crates/start-core/locales/release-notes.json'
  const labels = JSON.parse(text(join(root, catalog)))
  labels['release-notes.full-changelog'].pl_PL =
    '**[Zmieniony odsyłacz %{version}](%{url})**'
  write(catalog, JSON.stringify(labels))
  contains(
    JSON.parse(script('registry-notes').stdout).releaseNotes.pl_PL,
    'Zmieniony odsyłacz',
  )
  const pinned = run(
    process.execPath,
    [
      'scripts/changelog.mjs',
      'registry-notes',
      'projects/start-sdk',
      VERSION,
      '--ref',
      adopted,
    ],
    { encoding: 'utf8' },
  )
  assert.equal(pinned.status, 0, pinned.stderr)
  assert.deepEqual(JSON.parse(pinned.stdout), original)
  const mismatch = run(
    'bash',
    [
      '-c',
      `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"; PROJECT=start-sdk; KIND=npm; VERSION=1.2.3; COMMIT="${adopted}"
assert_metadata_matches_adopted
`,
    ],
    { encoding: 'utf8' },
  )
  assert.notEqual(mismatch.status, 0)
  contains(mismatch.stderr, 'release-note bundle')
})

test('archived localized notes keep the changelog link last and its source URL', t => {
  const { root, env, write, script, run, git, commit } = releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  write(`projects/start-sdk/release-notes/${VERSION}.md`, FRAMING)
  write(
    `projects/start-sdk/release-notes/${VERSION}.pl_PL.md`,
    'Wstęp.\n\n## Przedmowa\n\nUwaga.\n\n## Nowości\n\n<!-- highlights -->\n\n## Ważne\n\nWażna uwaga.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-fixed-feature.md',
    METADATA + '- Feature.\n',
  )
  commit()
  git('-c', 'tag.gpgsign=false', 'tag', 'start-sdk/v1.2.3')
  env.CHANGELOG_REF = 'HEAD'
  const pending = JSON.parse(script('registry-notes').stdout)
  const synced = run(process.execPath, ['scripts/changelog.mjs', 'sync'], {
    encoding: 'utf8',
  })
  assert.equal(synced.status, 0, synced.stderr)
  commit()
  const archived = JSON.parse(script('registry-notes').stdout)
  const polish = archived.releaseNotes.pl_PL
  contains(polish, '/blob/HEAD/projects/start-sdk/CHANGELOG.md#123')
  linkLast(polish)
  const pendingPolish = pending.releaseNotes.pl_PL
  contains(pendingPolish, 'Fragmenty pełnej listy zmian w wersji 1.2.3')
  contains(pendingPolish, '/tree/HEAD/projects/start-sdk/changelog')
  const withoutLink = body => body.replace(/^\*\*\[.+$/m, '<changelog link>')
  assert.equal(withoutLink(polish), withoutLink(pendingPolish))
})

test('CLI capability gates registry writes and CI uploads without gating pure notes', t => {
  const { root, env, write, script, run, nodeExecutable } = releaseFixture(t)
  write(`projects/start-wrt/release-notes/${VERSION}.md`, MAIN)
  const marker = join(root, 'registry-write')
  const helpCalls = join(root, 'help-calls')
  nodeExecutable(
    'bin/start-cli',
    `
import { appendFileSync, writeFileSync } from 'node:fs'
const args = process.argv.slice(2)
if (args.at(-1) === '--help') {
  appendFileSync(process.env.HELP_CALLS, JSON.stringify(args) + '\\n')
  console.log(process.env.CLI_HELP || 'English-only CLI help')
  process.exit(Number(process.env.HELP_STATUS || 0))
}
writeFileSync(process.env.WRITE_MARKER, JSON.stringify(args))
`,
  )
  Object.assign(env, {
    PATH: `${join(root, 'bin')}:${env.PATH}`,
    HELP_CALLS: helpCalls,
    WRITE_MARKER: marker,
  })
  script('notes', 'start-wrt')
  script('registry-notes', 'start-wrt')
  assert.equal(existsSync(marker), false)
  assert.equal(
    existsSync(helpCalls),
    false,
    'note compilation must not require an installed CLI',
  )
  const harness = `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"; PROJECT=start-wrt; KIND=wrt; VERSION=1.2.3
load_registry_index() { touch "${marker}"; }
enter_release_dir() { touch "${marker}"; }
`
  for (const command of [
    'refresh_registry_notes fixture-registry',
    'cmd_register',
  ]) {
    const rejected = run('bash', ['-c', harness + command], {
      encoding: 'utf8',
    })
    assert.notEqual(rejected.status, 0)
    contains(rejected.stderr, 'update it before publishing registry notes')
    assert.equal(existsSync(marker), false)
  }
  for (const [workflow, product, upload] of [
    ['startos-iso.yaml', 'start-os', 'Upload to S3'],
    ['start-wrt.yaml', 'start-wrt', 'Upload images to S3'],
  ]) {
    const source = text(join(REPO, '.github/workflows', workflow)).split(
      '\n  deploy:\n',
    )[1]
    const install = source
      .split('      - name: Install start-cli\n')[1]
      .split('\n      - name: ')[0]
    const command = `./scripts/manage-release.sh check-registry-notes-cli ${product}`
    contains(install, command)
    assert.ok(source.indexOf(command) < source.indexOf(`- name: ${upload}`))
    assert.ok(
      source.indexOf(command) <
        source.indexOf('- name: Drop the previous alpha build'),
    )
    const rejected = run('bash', ['-e', '-c', command], { encoding: 'utf8' })
    assert.notEqual(rejected.status, 0)
    contains(rejected.stderr, 'lacks LocaleString release notes')
  }
  env.CLI_HELP = 'Arguments:\n  <RELEASE_NOTES_OR_JSON>\n'
  script('check-registry-notes-cli', 'start-wrt')
  env.HELP_STATUS = '1'
  script('check-registry-notes-cli', 'start-wrt', false)
  assert.equal(existsSync(marker), false)
  for (const call of text(helpCalls).trim().split('\n'))
    assert.deepEqual(JSON.parse(call), [
      'registry',
      'os',
      'version',
      'add',
      '--help',
    ])
})

test('historical CRLF English notes keep the changelog link last', t => {
  const { write, script } = releaseFixture(t)
  write(
    `projects/start-sdk/release-notes/${VERSION}.md`,
    MAIN.replaceAll('\n', '\r\n'),
  )
  const notes = script().stdout
  contains(notes, 'Lede.\r\n')
  linkLast(notes)
})
