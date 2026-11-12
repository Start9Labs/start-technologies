import assert from 'node:assert/strict'
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO, fixture } from './fixtures.mjs'

const VERSION = '1.2.3'
const MAIN =
  'Lede.\n\n## Highlights\n\n- Feature.\n\n## Important\n\nFollow up.\n'
const FRAMING = 'Lede.\n\n## Important\n\nFollow up.\n'
const PRE_UPDATE = '## ⚠️ Before You Update\n\n> Prepare first.'
const wrapped = warning =>
  `<!-- pre-update -->\n\n${warning}\n\n<!-- /pre-update -->`
const METADATA = '---\nrelease-notes: true\n---\n\n'

function releaseFixture(t) {
  const f = fixture(t)
  const { root, env, write, run, git } = f
  mkdirSync(join(root, 'scripts'))
  for (const script of [
    'manage-release.sh',
    'changelog.mjs',
    'changelog-version.mjs',
    'release-notes-select.mjs',
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
  const published = join(root, '.published-notes.json')
  f.nodeExecutable(
    '.fallback-bin/gh',
    `import { existsSync, readFileSync } from 'node:fs'
const args = process.argv.slice(2)
const path = ${JSON.stringify(published)}
const notes = existsSync(path) ? JSON.parse(readFileSync(path)) : {}
if (args.some(arg => arg.includes('/releases/assets/')))
  process.stdout.write(JSON.stringify(notes[args.at(-1).split('/').pop()].notes))
else if (args[0] === 'api')
  for (const [id, { tag }] of Object.entries(notes)) console.log(tag + '\\t' + id)
else process.exit(1)
`,
  )
  Object.assign(env, {
    VERSION,
    CHANGELOG_REF: 'fixture-ref',
    GITHUB_REPOSITORY: 'fixture/repo',
    PATH: `${join(root, '.fallback-bin')}:${env.PATH}`,
  })
  function publishNotes(tag, notes) {
    const all = existsSync(published)
      ? JSON.parse(readFileSync(published, 'utf8'))
      : {}
    all[String(Object.keys(all).length + 1)] = { tag, notes }
    writeFileSync(published, JSON.stringify(all))
  }
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
  return { ...f, main, preUpdate, script, commit, blockPython, publishNotes }
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
  assert.equal(combined, wrapped(PRE_UPDATE) + '\n\n' + without)
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
    write(
      `projects/${project}/changelog/patch-security-internal.md`,
      '---\nrelease-notes: false\n---\n\n- Security correction.\n',
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
    if (project === 'start-os') {
      assert.equal(text(join(runnerTemp, 'release-security')), 'true\n')
      contains(deploy, '--git-hash "${{ github.sha }}" "${SECURITY_ARGS[@]}"')
    }
    const fragmentPath = `checkout-${project}/projects/${project}/changelog/patch-fixed-feature.md`
    write(fragmentPath, '---\nrelease-notes: invalid\n---\n\n- Invalid.\n')
    const invalid = compile()
    assert.notEqual(invalid.status, 0)
    contains(invalid.stderr, 'release-notes: true or false')
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
    `\n${wrapped(PRE_UPDATE)}\n\n${MAIN.replace('Feature.', 'Fixed feature.')}\n<details>`,
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
    '---\nrelease-notes: invalid\n---\n\n- Invalid.\n',
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
    contains(result.stderr, 'release-notes: true or false')
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
  const body = script('body').stdout
  contains(body, PRE_UPDATE)
  excludes(body, '## Highlights')
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
  function blocked(name = 'pre-update warnings') {
    const result = check()
    assert.notEqual(result.status, 0)
    contains(result.stderr, name)
    contains(result.stderr, 'differ')
  }
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  blocked()
  git('add', preUpdate)
  blocked()
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

test('adopted release accepts edited archived notes after fragments are consumed', t => {
  const { root, write, run, git, commit, publishNotes } = releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  const notesPath = `projects/start-sdk/release-notes/${VERSION}.md`
  write(notesPath, 'Lede.\n')
  const path = write(
    'projects/start-sdk/changelog/patch-fixed-public.md',
    METADATA + '- Released fix.\n',
  )
  commit()
  const adopted = git('rev-parse', 'HEAD').trim()
  git('-c', 'tag.gpgsign=false', 'tag', 'start-sdk/v1.2.3')
  publishNotes('start-sdk/v1.2.3', {
    en_US: 'Edited.\n',
    pl_PL: 'Zmienione.\n',
  })
  const synced = run(process.execPath, ['scripts/changelog.mjs', 'sync'])
  assert.equal(synced.status, 0, synced.stderr.toString())
  assert.equal(existsSync(path), false)
  assert.equal(text(join(root, notesPath)), 'Edited.\n')
  assert.equal(
    text(join(root, `projects/start-sdk/release-notes/${VERSION}.pl_PL.md`)),
    'Zmienione.\n',
  )
  commit()
  const command = `source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"; PROJECT=start-sdk; VERSION=1.2.3; COMMIT="${adopted}"
assert_metadata_matches_adopted`
  const check = () => run('bash', ['-c', command], { encoding: 'utf8' })
  const result = check()
  assert.equal(result.status, 0, result.stderr)
  const warning = `projects/start-sdk/release-notes/${VERSION}.pl_PL.pre-update.md`
  write(warning, 'Nowe ostrzeżenie.\n')
  const changedWarning = check()
  assert.notEqual(changedWarning.status, 0)
  contains(changedWarning.stderr, 'pre-update warnings differ')
  unlinkSync(join(root, warning))
  write(
    'projects/start-sdk/changelog/patch-fixed-other.md',
    METADATA + '- New change.\n',
  )
  assert.notEqual(check().status, 0)
})

test('create-gh-release drafts edited, translated notes at the tagged commit before the tag', t => {
  const { root, env, write, git, commit, nodeExecutable, run } =
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
  write(
    'projects/start-sdk/changelog/patch-changed-routine.md',
    '---\nrelease-notes: false\n---\n\n- Routine change.\n',
  )
  commit()
  const tagged = git('rev-parse', 'HEAD').trim()
  const commitProjects = message => {
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
      message,
    )
  }
  const remote = join(root, 'origin.git')
  git('init', '--bare', '-q', remote)
  git('remote', 'add', 'origin', remote)
  git('push', '-q', 'origin', 'HEAD:master')
  write(
    'projects/start-sdk/changelog/minor-added-later.md',
    METADATA + '- Later change.\n',
  )
  commitProjects('later')
  write(`projects/start-sdk/release-notes/${VERSION}.md`, 'Later lede.\n')
  write(
    `projects/start-sdk/release-notes/${VERSION}.pre-update.md`,
    PRE_UPDATE + '\nLater warning.\n',
  )
  const state = join(root, 'gh-state')
  const calls = join(root, 'gh-calls')
  const translations = join(root, 'claude-calls')
  nodeExecutable(
    'bin/gh',
    `import { appendFileSync, copyFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs'
const args = process.argv.slice(2)
const state = process.env.STATE
const flag = name => args[args.indexOf(name) + 1]
appendFileSync(process.env.CALLS, JSON.stringify(args) + '\\n')
if (args[1] === 'download') {
  if (!existsSync(state + '/release-notes.json')) process.exit(1)
  copyFileSync(state + '/release-notes.json', flag('--output'))
} else if (args[1] === 'view') {
  if (!existsSync(state + '/release')) process.exit(1)
  console.log(process.env.RELEASE_DRAFT || 'true')
} else if (args[1] === 'create' || args[1] === 'edit') {
  mkdirSync(state, { recursive: true })
  writeFileSync(state + '/release', '')
  if (args.includes('--notes-file')) copyFileSync(flag('--notes-file'), state + '/body.md')
} else if (args[1] === 'upload') {
  if (process.env.FAIL_UPLOAD) process.exit(1)
  for (const file of args.filter(arg => arg.startsWith('/')))
    copyFileSync(file, state + '/' + file.split('/').pop())
}
`,
  )
  nodeExecutable(
    'bin/claude',
    `import { appendFileSync, readFileSync } from 'node:fs'
const english = readFileSync(0, 'utf8').split('<english>\\n')[1].split('\\n</english>')[0]
const locale = process.argv.at(-1).match(/into the (\\S+) locale/)[1]
appendFileSync(process.env.TRANSLATIONS, locale + '\\n')
const output = process.env.DROP_COMMENTS ? english.replace(/<!--.*?-->/g, '') : english
process.stdout.write('Translated ' + locale + '.\\n\\n' + output + '\\n')
`,
  )
  const editor = write(
    'bin/edit-notes',
    `#!/bin/sh\nfor file; do case "$file" in */en_US.md) printf '\\nEdited by hand.\\n' >> "$file" ;; esac; done\n`,
  )
  chmodSync(editor, 0o755)
  Object.assign(env, {
    PATH: `${join(root, 'bin')}:${env.PATH}`,
    STATE: state,
    CALLS: calls,
    TRANSLATIONS: translations,
    COMMIT: tagged,
    VISUAL: editor,
  })
  const ghCalls = () =>
    text(calls)
      .trim()
      .split('\n')
      .map(line => JSON.parse(line))
  const manage = (command, keys = '\r', success = true) => {
    const result = run(
      'bash',
      [join(root, 'scripts/manage-release.sh'), command, 'start-sdk'],
      { encoding: 'utf8', input: keys },
    )
    if (success) assert.equal(result.status, 0, result.stderr)
    else assert.notEqual(result.status, 0)
    return result
  }

  manage('create-gh-release', ' \r')
  let recorded = ghCalls()
  assert.deepEqual(
    recorded.map(args => args[1]),
    ['download', 'view', 'create', 'upload'],
  )
  assert.ok(recorded[2].includes('--draft'))
  assert.equal(recorded[2][recorded[2].indexOf('--target') + 1], tagged)
  assert.ok(!recorded[2].includes('--verify-tag'))
  assert.equal(git('tag', '--list').trim(), '')
  const bundle = JSON.parse(text(join(state, 'release-notes.json')))
  assert.equal(
    bundle.en_US,
    'Lede.\n\n## Highlights\n\n- Routine change.\n\n- Fixed the released feature.\n\n## Important\n\nFollow up.\n\nEdited by hand.\n',
  )
  assert.deepEqual(Object.keys(bundle).sort(), [
    'de_DE',
    'en_US',
    'es_ES',
    'fr_FR',
    'pl_PL',
  ])
  assert.ok(bundle.pl_PL.startsWith('Translated pl_PL.\n\nLede.'))
  assert.ok(bundle.pl_PL.includes('Edited by hand.'))
  const body = text(join(state, 'body.md'))
  contains(body, 'Edited by hand.')
  contains(body, '<details>\n<summary>Full changelog</summary>\n\n## [1.2.3]')
  contains(body, '### Changed\n\n- Routine change.')
  contains(body, '### Fixed\n\n- Fixed the released feature.')
  excludes(body, 'Later change.')
  excludes(body, 'Later lede.')
  excludes(body, 'Later warning.')
  excludes(body, '**[Full changelog')
  const uploaded = text(join(state, 'CHANGELOG.md'))
  contains(uploaded, '## [1.2.3]')
  contains(uploaded, 'Old change.')
  excludes(uploaded, 'Later change.')

  git('-c', 'tag.gpgsign=false', 'tag', `start-sdk/v${VERSION}`, tagged)
  delete env.COMMIT
  unlinkSync(calls)
  unlinkSync(translations)
  env.VISUAL = 'true'
  manage('create-gh-release', '')
  recorded = ghCalls()
  assert.deepEqual(
    recorded.map(args => args[1]),
    ['download', 'view', 'edit', 'upload'],
  )
  assert.equal(existsSync(translations), false)
  assert.deepEqual(JSON.parse(text(join(state, 'release-notes.json'))), bundle)
  excludes(text(join(state, 'body.md')), 'Later warning.')
  assert.equal(text(join(state, 'CHANGELOG.md')), uploaded)

  env.COMMIT = git('rev-parse', 'HEAD').trim()
  const conflicting = manage('create-gh-release', '', false)
  contains(conflicting.stderr, `differs from the commit start-sdk/v${VERSION}`)
  assert.equal(text(join(state, 'CHANGELOG.md')), uploaded)
  delete env.COMMIT

  env.FORCE = '1'
  manage('create-gh-release', '')
  contains(text(join(state, 'CHANGELOG.md')), 'Later change.')
  delete env.FORCE

  const commenter = write(
    'bin/comment-notes',
    `#!/bin/sh\nfor file; do case "$file" in */en_US.md) printf '\\n<!-- keep -->\\n' >> "$file" ;; esac; done\n`,
  )
  chmodSync(commenter, 0o755)
  Object.assign(env, { VISUAL: commenter, DROP_COMMENTS: '1' })
  const mangled = manage('create-gh-release', '', false)
  contains(mangled.stderr, "notes' HTML comments differ from the English notes")
  assert.deepEqual(JSON.parse(text(join(state, 'release-notes.json'))), bundle)
  delete env.DROP_COMMENTS
  env.VISUAL = 'true'

  const translatedBeforeOptOut = text(translations)
  env.NO_TRANSLATE = '1'
  manage('create-gh-release', '')
  assert.deepEqual(JSON.parse(text(join(state, 'release-notes.json'))), {
    en_US: bundle.en_US,
  })
  assert.equal(text(translations), translatedBeforeOptOut)
  delete env.NO_TRANSLATE
  git('tag', '-d', `start-sdk/v${VERSION}`)
  env.COMMIT = tagged

  for (const [draft, expected] of [
    ['true', ['view', 'edit']],
    ['false', ['view']],
  ]) {
    env.RELEASE_DRAFT = draft
    unlinkSync(calls)
    manage('publish-gh-release')
    recorded = ghCalls()
    assert.deepEqual(
      recorded.map(args => args[1]),
      expected,
    )
    if (draft === 'true') assert.ok(recorded[1].includes('--draft=false'))
  }
  delete env.RELEASE_DRAFT

  rmSync(state, { recursive: true })
  env.FAIL_UPLOAD = '1'
  const tagged$ = join(root, 'tagged')
  const failed = run(
    'bash',
    [
      '-c',
      `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"
PROJECT=start-sdk; KIND=npm; VERSION=1.2.3; TAG=start-sdk/v1.2.3
cmd_pre_check() { :; }
cmd_tag() { touch "${tagged$}"; }
cmd_push() { :; }
cmd_release
`,
    ],
    { encoding: 'utf8', input: '\r' },
  )
  assert.notEqual(failed.status, 0)
  assert.equal(existsSync(tagged$), false)
  delete env.FAIL_UPLOAD

  rmSync(state, { recursive: true, force: true })
  unlinkSync(calls)
  write(
    'projects/start-sdk/changelog/patch-fixed-example.md',
    'invalid fragment\n',
  )
  commitProjects('invalid')
  env.COMMIT = git('rev-parse', 'HEAD').trim()
  manage('create-gh-release', '\r', false)
  assert.ok(
    !ghCalls().some(args => ['create', 'edit', 'upload'].includes(args[1])),
  )
})

test('resume requires a drafted release with notes, inline changelog and attachment', t => {
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
printf '%s %s' "\${DONE[create-gh-release]:-draft}" "\${DONE[publish-gh-release]:-publish}"
`
  const summary = '<summary>Full changelog</summary>'
  const assets = [{ name: 'CHANGELOG.md' }, { name: 'release-notes.json' }]
  for (const [metadata, expected] of [
    [null, 'draft publish'],
    [{ isDraft: false, assets: [], body: '' }, 'draft publish'],
    [{ isDraft: false, assets, body: MAIN }, 'draft publish'],
    [{ isDraft: false, assets: [], body: summary }, 'draft publish'],
    [{ isDraft: false, assets: [assets[0]], body: summary }, 'draft publish'],
    [{ isDraft: true, assets, body: summary }, '1 publish'],
    [{ isDraft: false, assets, body: summary }, '1 1'],
  ]) {
    env.RELEASE_METADATA = metadata ? JSON.stringify(metadata) : ''
    const result = run('bash', ['-c', command], { encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    assert.equal(result.stdout, expected)
  }
})

test('the notes editor is VISUAL, EDITOR, then sensible-editor', t => {
  const { root, env, run, nodeExecutable } = releaseFixture(t)
  nodeExecutable('bin/sensible-editor', '')
  const editor = vars => {
    const result = run(
      'bash',
      [
        '-c',
        `source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh); notes_editor`,
      ],
      {
        encoding: 'utf8',
        env: { ...env, PATH: `${join(root, 'bin')}:${env.PATH}`, ...vars },
      },
    )
    assert.equal(result.status, 0, result.stderr)
    return result.stdout.trim()
  }
  assert.equal(editor({ VISUAL: 'code --wait', EDITOR: 'nano' }), 'code --wait')
  assert.equal(editor({ VISUAL: '', EDITOR: 'nano' }), 'nano')
  assert.equal(editor({ VISUAL: '', EDITOR: '' }), 'sensible-editor')
})

test('untagged draft resume checks the commit after artifact adoption', t => {
  const { root, env, write, run, git, commit, nodeExecutable } =
    releaseFixture(t)
  git('init', '-q')
  commit()
  const original = git('rev-parse', 'HEAD').trim()
  write('projects/unrelated/file', 'Later build')
  commit()
  const newer = git('rev-parse', 'HEAD').trim()
  nodeExecutable('bin/gh', 'console.log(process.env.DRAFT_SHA)')
  env.PATH = `${join(root, 'bin')}:${env.PATH}`
  env.DRAFT_SHA = original

  for (const selected of [original, newer]) {
    const result = run(
      'bash',
      [
        '-c',
        `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"
PROJECT=start-tunnel; KIND=deb; VERSION=1.2.3; TAG=start-tunnel/v1.2.3
COMMIT=${original}
cmd_pre_check() { DONE[create-gh-release]=1; }
cmd_pull_alpha() { COMMIT=${selected}; }
cmd_tag() { echo tagged; }
cmd_publish_gh_release() { echo published; }
cmd_push() { :; }
cmd_sign() { :; }
cmd_release
`,
      ],
      { encoding: 'utf8' },
    )
    if (selected === original) {
      assert.equal(result.status, 0, result.stderr)
      contains(result.stdout, 'tagged\npublished')
    } else {
      assert.notEqual(result.status, 0)
      contains(result.stderr, `Draft start-tunnel/v1.2.3 targets ${original}`)
      contains(result.stderr, `selected artifacts were built from ${newer}`)
      excludes(result.stdout, 'tagged')
      excludes(result.stdout, 'published')
    }
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
  write(`projects/start-os/release-notes/${version}.pl_PL.md`, 'Archived.\n')
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
    `mkdir = true
rm = true
ln = true
ls-files = $(wildcard $(1)/*)
cp = $(if $(filter %/release-notes.md,$(2)),printf '%s\\n' "$(2)" >> "${copies}"; cp "$(1)" "$(2)",true)
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
  excludes(inputs.stdout, 'pl_PL')
  excludes(inputs.stdout, 'pre-update')
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
  assert.deepEqual(text(copies).trim().split('\n'), [
    join(stage, 'release-notes.md'),
  ])
  assert.equal(existsSync(join(stage, 'release-notes')), false)
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
    '---\nrelease-notes: false\n---\n\n- Internal detail.\n',
  )
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
  excludes(registry, 'Internal detail.')
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
cp = $(if $(filter %/release-notes.md,$(2)),$(call remote-copy,$(1),$(2)),true)
`,
  )
  unlinkSync(join(stage, 'release-notes.md'))
  const remote = install(remoteHarness)
  assert.equal(remote.status, 0, remote.stderr)
  assert.equal(text(join(stage, 'release-notes.md')), generatedOnly)
  assert.deepEqual(renderTemps(), [])

  const copiedBeforeFailure = text(copies)
  write(
    'projects/start-os/changelog/patch-fixed-public.md',
    '---\nrelease-notes: invalid\n---\n\n- Invalid.\n',
  )
  const failed = install()
  assert.notEqual(failed.status, 0)
  contains(failed.stderr, 'release-notes: true or false')
  assert.equal(text(join(stage, 'release-notes.md')), generatedOnly)
  assert.equal(text(copies), copiedBeforeFailure)
  assert.deepEqual(renderTemps(), [])
})

test('registry payload composes edited locales, warnings, fallbacks and links', t => {
  const f = releaseFixture(t)
  const { root, env, write, script, git, commit } = f
  delete env.CHANGELOG_REF
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  write(`projects/start-sdk/release-notes/${VERSION}.md`, FRAMING)
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  write(
    `projects/start-sdk/release-notes/${VERSION}.pl_PL.pre-update.md`,
    '## Przed aktualizacją\n\nPolskie ostrzeżenie.\n',
  )
  write(`projects/start-sdk/release-notes/${VERSION}.de_DE.pre-update.md`, '')
  write(
    `projects/start-sdk/release-notes/${VERSION}.fr_FR.pre-update.md`,
    '## Avant la mise à jour\n\nAttention.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-added-feature.md',
    METADATA + '- Feature.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-changed-internal.md',
    '---\nrelease-notes: false\n---\n\n- Internal.\n',
  )
  commit()
  const snapshot = git('rev-parse', 'HEAD').trim()
  const payload = () => JSON.parse(script('registry-notes').stdout)
  const compiled = payload().releaseNotes
  assert.deepEqual(Object.keys(compiled).sort(), [
    'de_DE',
    'en_US',
    'fr_FR',
    'pl_PL',
  ])
  assert.equal(script('notes').stdout, compiled.en_US)
  contains(compiled.en_US, PRE_UPDATE)
  contains(compiled.en_US, 'Lede.\n\n## Highlights\n\n- Feature.')
  contains(compiled.en_US, '/releases/tag/start-sdk/v1.2.3')
  linkLast(compiled.en_US)
  contains(compiled.pl_PL, 'Polskie ostrzeżenie.')
  contains(compiled.pl_PL, '- Feature.')
  contains(compiled.pl_PL, 'Pełna lista zmian')
  excludes(compiled.pl_PL, 'Prepare first.')
  linkLast(compiled.pl_PL)
  contains(compiled.de_DE, PRE_UPDATE)
  contains(compiled.de_DE, 'Vollständiges Änderungsprotokoll')
  contains(compiled.fr_FR, 'Attention.')
  for (const body of Object.values(compiled)) excludes(body, 'Internal.')

  const edited = write(
    'edited.json',
    JSON.stringify({
      en_US: 'Edited English.\n',
      pl_PL: 'Edytowane.\n',
      es_ES: 'Editado.\n',
    }),
  )
  env.NOTES_FILE = edited
  const notes = payload().releaseNotes
  assert.deepEqual(Object.keys(notes).sort(), [
    'de_DE',
    'en_US',
    'es_ES',
    'fr_FR',
    'pl_PL',
  ])
  assert.ok(
    notes.en_US.startsWith(`${wrapped(PRE_UPDATE)}\n\nEdited English.\n`),
  )
  contains(
    notes.pl_PL,
    'Polskie ostrzeżenie.\n\n<!-- /pre-update -->\n\nEdytowane.',
  )
  contains(notes.es_ES, `${wrapped(PRE_UPDATE)}\n\nEditado.`)
  contains(notes.es_ES, 'Registro completo de cambios')
  contains(notes.fr_FR, 'Attention.\n\n<!-- /pre-update -->\n\nEdited English.')
  contains(notes.de_DE, 'Edited English.')
  for (const body of Object.values(notes)) {
    excludes(body, 'Feature.')
    linkLast(body)
  }
  const github = script('body').stdout
  contains(github, PRE_UPDATE)
  contains(github, 'Edited English.')
  excludes(github, 'Edytowane.')
  excludes(github, 'Full changelog for')
  delete env.NOTES_FILE

  env.CHANGELOG_REF = snapshot
  const pending = payload()
  contains(pending.releaseNotes.en_US, 'Full changelog fragments for v1.2.3')
  contains(
    pending.releaseNotes.pl_PL,
    `/tree/${snapshot}/projects/start-sdk/changelog`,
  )
  env.NOTES_REF = snapshot
  write(
    `projects/start-sdk/release-notes/${VERSION}.pl_PL.pre-update.md`,
    'Later warning.\n',
  )
  write(
    'projects/start-sdk/changelog/patch-added-feature.md',
    METADATA + '- Later feature.\n',
  )
  assert.deepEqual(
    payload(),
    pending,
    'publishing from a ref uses its notes and warnings',
  )
  delete env.NOTES_REF
  delete env.CHANGELOG_REF
})

test('registry and GitHub notes include every Security fragment without summary files', t => {
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
    'projects/start-sdk/changelog/patch-security-internal.md',
    '---\nrelease-notes: false\n---\n\n- Unselected security detail.\n',
  )
  write(`projects/start-sdk/release-notes/${VERSION}.pre-update.md`, PRE_UPDATE)
  commit()
  const notes = JSON.parse(script('registry-notes').stdout).releaseNotes
  contains(notes, '## Highlights\n\n- Feature.')
  contains(
    notes,
    '## Security\n\n- Security fix.\n\n- Unselected security detail.',
  )
  contains(notes, PRE_UPDATE)
  linkLast(notes)
  assert.ok(
    !existsSync(join(root, `projects/start-sdk/release-notes/${VERSION}.md`)),
  )
  contains(script('body').stdout, '## Security\n\n- Security fix.')
  write(
    `projects/start-sdk/release-notes/${VERSION}.md`,
    '## Important\n\nKeep this notice.\n',
  )
  const framed = JSON.parse(script('registry-notes').stdout).releaseNotes
  assert.ok(framed.indexOf('## Security') < framed.indexOf('## Important'))
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
    `projects/start-sdk/release-notes/${VERSION}.pl_PL.pre-update.md`,
    'Ostrzeżenie.\n',
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
    `projects/start-wrt/release-notes/${VERSION}.pl_PL.pre-update.md`,
    'Polskie ostrzeżenie.\n',
  )
  const expected = JSON.parse(script('registry-notes', 'start-wrt').stdout)
  assert.equal(typeof expected.releaseNotes, 'object')
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
    assert.deepEqual(args.slice(1, 6), [
      'registry',
      'os',
      'version',
      'add',
      VERSION,
    ])
    assert.deepEqual(JSON.parse(args[6]), expected.releaseNotes)
    assert.equal(args.length, 8)
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
      .replaceAll('${{ github.sha }}', 'fixture-sha')
    const runnerTemp = join(root, 'runner')
    write('runner/release-notes.json', JSON.stringify(expected))
    write('runner/release-security', 'true\n')
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
    const notesIndex = workflow === 'startos-iso.yaml' ? 9 : 6
    assert.deepEqual(JSON.parse(args[notesIndex]), expected.releaseNotes)
    assert.equal(args.length, notesIndex + 2)
    if (workflow === 'startos-iso.yaml')
      assert.deepEqual(args.slice(5, 8), [
        '--git-hash',
        'fixture-sha',
        '--security',
      ])
    else {
      excludes(args.join(' '), '--git-hash')
      excludes(args.join(' '), '--security')
    }
  }
})

test('compiler failures prevent registry writes and warning edits block alpha adoption', t => {
  const { root, write, run, git, commit } = releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  const framing = `projects/start-wrt/release-notes/${VERSION}.md`
  write(framing, FRAMING)
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
  contains(changed.stderr, 'pre-update warnings differ')
  unlinkSync(join(root, warning))
  const marker = join(root, 'registry-write')
  write(framing, 'Lede.\n\n## Highlights\n\n- Invalid authored highlights.\n')
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
    contains(failed.stderr, 'remove ## Highlights')
    assert.equal(existsSync(marker), false)
  }
})

test('registry source refs pin shared link translations', t => {
  const { root, env, write, script, run, git, commit } = releaseFixture(t)
  delete env.CHANGELOG_REF
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  write(`projects/start-sdk/release-notes/${VERSION}.md`, FRAMING)
  write(
    `projects/start-sdk/release-notes/${VERSION}.pl_PL.pre-update.md`,
    'Ostrzeżenie.\n',
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
})

test('edited notes keep the changelog link last and follow its source URL', t => {
  const { root, env, write, script, run, git, commit, publishNotes } =
    releaseFixture(t)
  git('init', '-q')
  git('remote', 'add', 'origin', root)
  write(`projects/start-sdk/release-notes/${VERSION}.md`, FRAMING)
  write(
    'projects/start-sdk/changelog/patch-fixed-feature.md',
    METADATA + '- Feature.\n',
  )
  commit()
  git('-c', 'tag.gpgsign=false', 'tag', 'start-sdk/v1.2.3')
  const bundle = {
    en_US: 'Edited.\n',
    pl_PL: 'Wstęp.\n\n## Nowości\n\n- Funkcja.\n',
  }
  env.NOTES_FILE = write('edited.json', JSON.stringify(bundle))
  env.CHANGELOG_REF = 'HEAD'
  const pending = JSON.parse(script('registry-notes').stdout).releaseNotes
  publishNotes('start-sdk/v1.2.3', bundle)
  const synced = run(process.execPath, ['scripts/changelog.mjs', 'sync'], {
    encoding: 'utf8',
  })
  assert.equal(synced.status, 0, synced.stderr)
  commit()
  const archived = JSON.parse(script('registry-notes').stdout).releaseNotes
  contains(archived.pl_PL, '/blob/HEAD/projects/start-sdk/CHANGELOG.md#123')
  contains(pending.pl_PL, 'Fragmenty pełnej listy zmian w wersji 1.2.3')
  contains(pending.pl_PL, '/tree/HEAD/projects/start-sdk/changelog')
  for (const notes of [archived, pending])
    for (const body of Object.values(notes)) linkLast(body)
  const withoutLink = body => body.replace(/^\*\*\[.+$/m, '<changelog link>')
  assert.equal(withoutLink(archived.pl_PL), withoutLink(pending.pl_PL))
  delete env.NOTES_FILE
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
    const check = source
      .split('      - name: Check start-cli registry notes support\n')[1]
      .split('\n      - name: ')[0]
    const command = `./scripts/manage-release.sh check-registry-notes-cli ${product}`
    contains(check, command)
    assert.ok(source.indexOf(command) < source.indexOf(`- name: ${upload}`))
    assert.ok(
      source.indexOf(command) <
        source.indexOf('- name: Drop the previous alpha build'),
    )
    const rejected = run('bash', ['-e', '-c', command], { encoding: 'utf8' })
    assert.notEqual(rejected.status, 0)
    contains(rejected.stderr, 'lacks headline-free LocaleString release notes')
  }
  env.CLI_HELP = 'Arguments:\n  <HEADLINE>\n  <RELEASE_NOTES_OR_JSON>\n'
  script('check-registry-notes-cli', 'start-wrt', false)
  assert.equal(existsSync(marker), false)
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

for (const [project, kind] of [
  ['start-os', 'os'],
  ['start-wrt', 'wrt'],
]) {
  for (const [name, indexCommand] of [
    ['unavailable index', 'return 42'],
    ['missing version', `printf '%s\\n' '{"versions":{}}'`],
  ]) {
    test(`cosign-registry rejects ${name} for ${project}`, t => {
      const { root, env, run } = releaseFixture(t)
      env.HOME = join(root, 'home')
      const result = run(
        'bash',
        [
          '-c',
          `
source <(awk '/^SUBCOMMAND=/{exit} {print}' scripts/manage-release.sh)
REPO_ROOT="$PWD"; PROJECT=${project}; KIND=${kind}; VERSION=${VERSION}; TAG=${project}/v${VERSION}
mkdir -p "$(release_dir)"
start-cli() { ${indexCommand}; }
cmd_cosign_registry
`,
        ],
        { encoding: 'utf8' },
      )
      assert.notEqual(result.status, 0)
      contains(result.stderr, `has no ${VERSION} entry`)
      excludes(result.stdout, 'Done. Developer signatures added')
    })
  }
}

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
