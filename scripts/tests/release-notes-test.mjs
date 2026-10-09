import assert from 'node:assert/strict'
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  unlinkSync,
} from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO, fixture } from './fixtures.mjs'

const VERSION = '1.2.3'
const MAIN =
  'Lede.\n\n## Highlights\n\n- Feature.\n\n## Important\n\nFollow up.\n'
const PRE_UPDATE = '## ⚠️ Before You Update\n\n> Prepare first.'

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

test('optional companion', t => {
  const { script, write } = releaseFixture(t)
  const without = script().stdout
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, '')
  assert.equal(script().stdout, without)
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  const combined = script().stdout
  assert.equal(combined, PRE_UPDATE + '\n\n' + without)
  assert.ok(
    combined.indexOf('Full changelog') < combined.indexOf('## Important'),
  )
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
  unlinkSync(join(root, '.prettierrc.json'))
  write('package.json', '{"version":"0.4.0.3"}')
  write(
    'projects/start-wrt/backend/ctrl/Cargo.toml',
    '[package]\nname = "ctrl"\nversion = "1.2.3"\n',
  )
  const cases = [
    ['start-os', 'startos-iso.yaml', '0.4.0.3', '0403'],
    ['start-wrt', 'start-wrt.yaml', VERSION, '123'],
  ]
  for (const [project, , version] of cases) {
    write(`projects/${project}/release-notes/${version}.md`, MAIN)
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
    assert.equal(existsSync(join(checkout, '.prettierrc.json')), false)
    assert.equal(existsSync(join(checkout, 'node_modules')), false)
    if (project === 'start-wrt')
      assert.equal(existsSync(join(checkout, 'package.json')), false)
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
    contains(derived, '- Feature.')
    contains(derived, `/blob/${ref}/projects/${project}/CHANGELOG.md#${anchor}`)
    env.VERSION = version
    unlinkSync(
      join(
        checkout,
        project === 'start-os'
          ? 'package.json'
          : 'projects/start-wrt/backend/ctrl/Cargo.toml',
      ),
    )
    assert.equal(script('notes', project, true, checkout).stdout, derived)
    delete env.VERSION
  }
  assert.equal(
    existsSync(marker),
    false,
    'deploy release caller invoked Python',
  )
})

test('release links rendered changelog', t => {
  const { env, write, script } = releaseFixture(t)
  delete env.CHANGELOG_REF
  for (const [project, version, anchor] of [
    ['start-sdk', '1.2.3', '123'],
    ['start-os', '0.4.0.3', '0403'],
    ['start-os', '0.4.1', '041'],
    ['start-sdk', '1.2.3-RC.1+Build.2', '123-rc1build2'],
  ]) {
    env.VERSION = version
    write(`projects/${project}/release-notes/${version}.md`, MAIN)
    const output = script('notes', project).stdout
    contains(output, `/blob/master/projects/${project}/CHANGELOG.md#${anchor}`)
    excludes(output, '/releases/download/')
  }
})

test('source links survive fragment consumption', t => {
  const { env, write, script, git, commit } = releaseFixture(t)
  git('init', '-q')
  write(
    'projects/start-sdk/changelog/patch-fixed-example.md',
    '- Fixed feature.\n',
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

test('GitHub and registry share composition', t => {
  const { write, script } = releaseFixture(t)
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  const notes = script().stdout
  const body = script('body').stdout
  assert.ok(body.endsWith('\n' + notes + '\n'))
  assert.equal(body.split(PRE_UPDATE).length - 1, 1)
})

test('missing main fails even with companion', t => {
  const { main, write, script } = releaseFixture(t)
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  unlinkSync(main)
  contains(script('notes', 'start-sdk', false).stderr, 'No release notes')
  contains(script('body', 'start-sdk', false).stderr, 'No release notes')
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
    contains(result.stderr, `${name} differs`)
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
  write('projects/start-sdk/CHANGELOG.md', '# Generated history\n')
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

test('release uploads changelog from tag before publication', t => {
  const { root, env, write, script, git, commit, nodeExecutable } =
    releaseFixture(t)
  git('init', '-q')
  write(
    'projects/start-sdk/CHANGELOG.md',
    '# Changelog\n\n## [1.2.2]\n\n- Old change.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-fixed-example.md',
    '- Fixed the released feature.\n',
  )
  commit()
  git('-c', 'tag.gpgsign=false', 'tag', 'start-sdk/v1.2.3')
  const remote = join(root, 'origin.git')
  git('init', '--bare', '-q', remote)
  git('remote', 'add', 'origin', remote)
  git('push', '-q', 'origin', 'HEAD:master', '--tags')
  write(
    'projects/start-sdk/changelog/minor-added-later.md',
    '- Later change.\n',
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
  const captured = join(root, 'uploaded.md')
  const calls = join(root, 'gh-calls')
  nodeExecutable(
    'bin/gh',
    `import { appendFileSync, copyFileSync } from 'node:fs'
const args = process.argv.slice(2)
appendFileSync(process.env.CALLS, args.join(' ') + '\\n')
if (args[0] === 'release' && args[1] === 'view')
  process.exit(process.env.EXISTING_RELEASE ? 0 : 1)
if (args[0] === 'release' && args[1] === 'upload')
  copyFileSync(args.find(arg => arg.endsWith('/CHANGELOG.md')), process.env.CAPTURE)
`,
  )
  Object.assign(env, {
    PATH: `${join(root, 'bin')}:${env.PATH}`,
    CAPTURE: captured,
    CALLS: calls,
  })
  script('create-gh-release')
  contains(text(calls), 'release create')
  unlinkSync(calls)
  env.EXISTING_RELEASE = '1'
  script('create-gh-release')
  contains(text(calls), 'release edit')
  const uploaded = text(captured)
  contains(uploaded, '## [1.2.3]')
  contains(uploaded, 'Fixed the released feature.')
  contains(uploaded, 'Old change.')
  excludes(uploaded, 'Later change.')
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

test('StartOS registry and packaged welcome', t => {
  const { root, env, write, script, run } = releaseFixture(t)
  const version = '0.4.0.2'
  const source = join(REPO, 'projects/start-os/release-notes')
  for (const name of [`${version}.md`, `${version}.pre-update.md`])
    write(
      `projects/start-os/release-notes/${name}`,
      readFileSync(join(source, name)),
    )
  env.VERSION = version
  const combined = script('notes', 'start-os').stdout
  contains(combined, '## ⚠️ Before You Update')
  contains(combined, 'only way to update from 0.3.5.1')
  contains(combined, text(join(source, `${version}.pre-update.md`)).trim())
  const stage = join(root, 'image/usr/lib/startos')
  mkdirSync(stage, { recursive: true })
  const versionFile = write('VERSION.txt', version)
  const harness = write(
    'install.mk',
    `mkdir = true
rm = true
ln = true
cp = $(if $(filter %/release-notes.md,$(2)),cp "$(1)" "$(2)",true)
include ${REPO}/projects/start-os/build.mk
`,
  )
  const result = run(
    'make',
    [
      '-f',
      harness,
      'start-os-install',
      'STARTOS_TARGETS=',
      'PLATFORM=x86_64',
      `VERSION_FILE=${versionFile}`,
      `DESTDIR=${root}/image`,
      'ENVIRONMENT=',
    ],
    { cwd: REPO, encoding: 'utf8' },
  )
  assert.equal(result.status, 0, result.stderr)
  const packaged = text(join(stage, 'release-notes.md'))
  assert.equal(packaged, text(join(source, `${version}.md`)))
  excludes(packaged, 'Before You Update')
  excludes(packaged, 'only way to update')
  contains(packaged, '## Highlights')
})
