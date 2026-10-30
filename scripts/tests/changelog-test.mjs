import assert from 'node:assert/strict'
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmdirSync,
  symlinkSync,
  unlinkSync,
  writeFileSync,
} from 'node:fs'
import { basename, join } from 'node:path'
import { test } from 'node:test'
import { fixture, REPO } from './fixtures.mjs'

const SCRIPT = join(REPO, 'scripts/changelog.mjs')
const PRETTIER = JSON.parse(readFileSync(join(REPO, 'package.json')))
  .devDependencies.prettier
const PRODUCT = 'projects/start-os'
const HISTORY = Buffer.from(
  '# Changelog\r\n\r\n## [0.3.0]\r\n\r\n### Fixed\r\n\r\n- Historical bytes.\r\n',
)

function compilerFixture(t) {
  const f = fixture(t)
  const { root, env, run, git, write } = f
  git('init', '-q', '-b', 'master')
  git('config', 'user.name', 'Fixture')
  git('config', 'user.email', 'fixture@example.com')
  git('config', 'commit.gpgsign', 'false')
  git('config', 'tag.gpgsign', 'false')
  git('config', 'core.hooksPath', '/dev/null')
  git('remote', 'add', 'origin', root)
  const fragments = join(root, PRODUCT, 'changelog')
  mkdirSync(fragments, { recursive: true })
  const history = write(`${PRODUCT}/CHANGELOG.md`, HISTORY)
  write('.prettierrc.json', readFileSync(join(REPO, '.prettierrc.json')))
  write(
    'package.json',
    JSON.stringify({ devDependencies: { prettier: PRETTIER } }),
  )
  function commit(date) {
    git('add', '.')
    const result = run('git', ['commit', '--allow-empty', '-qm', 'fixture'], {
      env: date
        ? { ...env, GIT_AUTHOR_DATE: date, GIT_COMMITTER_DATE: date }
        : env,
    })
    assert.equal(result.status, 0, result.stderr.toString())
    return git('rev-parse', 'HEAD').trim()
  }
  function tag(version, annotated = false) {
    const name = `start-os/v${version}`
    git('tag', ...(annotated ? ['-a', '-m', 'fixture'] : []), name)
    return name
  }
  function fragment(
    name = 'patch-fixed-example.md',
    body = '- Fixed example.\n',
    userFacing = true,
  ) {
    return write(
      `${PRODUCT}/changelog/${name}`,
      userFacing === null
        ? body
        : Buffer.concat([
            Buffer.from(`---\nuser-facing: ${userFacing}\n---\n\n`),
            Buffer.from(body),
          ]),
    )
  }
  function cli(args, success = true) {
    const result = run(process.execPath, [SCRIPT, ...args])
    if (success) assert.equal(result.status, 0, result.stderr.toString())
    else assert.notEqual(result.status, 0)
    return result
  }
  function render(version = '0.4.0.1', ref) {
    return cli(['render', PRODUCT, version, ...(ref ? ['--ref', ref] : [])])
      .stdout
  }
  function manifest(version, product = PRODUCT) {
    const name = basename(product)
    const path =
      name === 'start-os'
        ? 'package.json'
        : name === 'start-sdk'
          ? `${product}/package.json`
          : name === 'start-wrt'
            ? `${product}/backend/ctrl/Cargo.toml`
            : `${product}/Cargo.toml`
    write(
      path,
      path.endsWith('.json')
        ? JSON.stringify({ version, devDependencies: { prettier: PRETTIER } })
        : `[package]\nversion = "${version}"\n`,
    )
  }
  commit()
  return {
    ...f,
    fragments,
    history,
    commit,
    tag,
    fragment,
    cli,
    render,
    manifest,
  }
}

function contains(bytes, value) {
  assert.ok(
    bytes.includes(value),
    `Missing ${JSON.stringify(value.toString())}`,
  )
}
function excludes(bytes, value) {
  assert.ok(
    !bytes.includes(value),
    `Unexpected ${JSON.stringify(value.toString())}`,
  )
}
function count(bytes, value) {
  return bytes.toString().split(value).length - 1
}

test('deterministic grouping and multiline bullets', t => {
  const { fragment, render } = compilerFixture(t)
  fragment('major-security-z.md', '- Security.\n')
  fragment(
    'patch-added-z.md',
    '- Zed.\n  Continued **Markdown**.\n\n- Second.\n',
  )
  fragment('minor-added-a.md', '* Alpha.\n')
  fragment('patch-removed-a.md', '- Removed.\n')
  fragment('patch-deprecated-a.md', '- Deprecated.\n')
  fragment('patch-changed-a.md', '- Changed.\n')
  fragment('patch-fixed-a.md', '- Fixed.\n')
  const expected = Buffer.from(
    '# Changelog\n\n## [0.4.0.1]\n\n' +
      '### Added\n\n- Alpha.\n\n* Zed.\n  Continued **Markdown**.\n\n* Second.\n\n' +
      '### Changed\n\n- Changed.\n\n### Deprecated\n\n- Deprecated.\n\n' +
      '### Removed\n\n- Removed.\n\n### Fixed\n\n- Fixed.\n\n' +
      '### Security\n\n- Security.\n\n' +
      HISTORY.subarray(HISTORY.indexOf('## '))
        .toString()
        .replaceAll('\r\n', '\n'),
  )
  assert.deepEqual(render(), expected)
  assert.deepEqual(render(), expected)
})

test('assembled loose list is formatted and sync is idempotent', t => {
  const { history, fragment, commit, tag, render, cli, git } =
    compilerFixture(t)
  writeFileSync(history, '')
  fragment('patch-fixed-a.md', '- First.\n\n- Second.\n')
  fragment('patch-fixed-b.md', '- Third.\n')
  commit()
  const release = tag('0.4.0.1')
  const rendered = render('0.4.0.1', release)
  assert.deepEqual(
    rendered,
    Buffer.from(
      '## [0.4.0.1]\n\n### Fixed\n\n- First.\n\n- Second.\n\n- Third.\n',
    ),
  )
  cli(['sync'])
  assert.deepEqual(readFileSync(history), rendered)
  cli(['sync'])
  assert.deepEqual(readFileSync(history), rendered)
  commit()
  fragment('patch-fixed-a.md', '- Genuinely changed.\n\n- Second.\n')
  fragment('patch-fixed-b.md', '- Third.\n')
  commit()
  git('tag', '-f', release)
  cli(['sync'], false)
  assert.deepEqual(readFileSync(history), rendered)
})

test('formatted marker changes are idempotent', t => {
  const { history, fragment, commit, tag, cli, git } = compilerFixture(t)
  writeFileSync(history, '')
  fragment('patch-fixed-a.md', '* Alpha.\n')
  fragment('patch-fixed-b.md', '- Beta.\n')
  commit()
  tag('0.4.0.1')
  cli(['sync'])
  const expected = Buffer.from(
    '## [0.4.0.1]\n\n### Fixed\n\n- Alpha.\n\n* Beta.\n',
  )
  assert.deepEqual(readFileSync(history), expected)
  commit()
  cli(['sync'])
  assert.deepEqual(readFileSync(history), expected)
  assert.equal(git('diff', 'HEAD'), '')
})

test('invalid UTF-8 is rejected', t => {
  const { fragment, cli, commit, tag } = compilerFixture(t)
  fragment(
    undefined,
    Buffer.concat([
      Buffer.from('- Invalid '),
      Buffer.from([0xff]),
      Buffer.from('.\n'),
    ]),
  )
  cli(['validate', PRODUCT], false)
  commit()
  const release = tag('0.4.0.1')
  cli(['render', PRODUCT, '0.4.0.1', '--ref', release], false)
})

test('tagged large blob and render stdout are not truncated', t => {
  const { fragment, commit, tag, render } = compilerFixture(t)
  const body = `- ${'x'.repeat(1024 * 1024 + 100)}.\n`
  fragment(undefined, body)
  commit()
  const release = tag('0.4.0.1')
  contains(render('0.4.0.1', release), Buffer.from(body))
})

test('noncompiling commands do not require formatter configuration', t => {
  const { root, manifest, write, fragment, cli } = compilerFixture(t)
  unlinkSync(join(root, '.prettierrc.json'))
  manifest('0.4.0.3')
  write('package.json', '{"version":"0.4.0.3"}')
  fragment()
  cli(['projects'])
  cli(['version', PRODUCT])
  cli(['validate', PRODUCT])
  cli(['check-version', PRODUCT])
  cli(['changed', 'HEAD'])
})

test('command help', async t => {
  const { cli } = compilerFixture(t)
  for (const args of [['--help'], ['-h'], ['render', '--help']])
    await t.test(JSON.stringify(args), () => {
      const output = cli(args).stdout
      contains(output, 'render PRODUCT VERSION [--ref REF]')
      contains(output, 'check-version PRODUCT [VERSION]')
    })
})

test('invalid filenames', async t => {
  const { fragment, cli } = compilerFixture(t)
  for (const name of [
    'fixed-name.md',
    'patch-Fixed-name.md',
    'patch-other-name.md',
    'patch-fixed-.md',
    'patch-fixed-name.txt',
    'patch-fixed-name space.md',
  ])
    await t.test(name, () => {
      const path = fragment(name)
      cli(['validate', PRODUCT], false)
      unlinkSync(path)
    })
})

test('rejected bodies', async t => {
  const { fragment, cli } = compilerFixture(t)
  for (const body of [
    '',
    ' \n',
    'plain text',
    '- Item.\n\n## Heading\n',
    '- Item.\nHeading\n=======\n',
    '- Item.\n  ### Heading\n',
  ])
    await t.test(JSON.stringify(body), () => {
      fragment(undefined, body)
      cli(['validate', PRODUCT], false)
    })
})

test('section extracts exactly one compiled version', t => {
  const { run } = compilerFixture(t)
  const history = Buffer.from(
    '# Changelog\n\n## [1.2.30]\n\n- Newer.\n\n' +
      '## [1.2.3]\n\n### Fixed\n\n- This release.\n\n' +
      '## [1.2.2]\n\n- Older.\n',
  )
  const section = version => {
    const result = run(process.execPath, [SCRIPT, 'section', version], {
      input: history,
    })
    assert.equal(result.status, 0, result.stderr.toString())
    return result.stdout
  }
  assert.equal(
    section('1.2.3').toString(),
    '## [1.2.3]\n\n### Fixed\n\n- This release.\n\n',
  )
  assert.equal(section('1.2.2').toString(), '## [1.2.2]\n\n- Older.\n')
  assert.equal(section('1.2.4').length, 0)
})

test('empty directory is valid', t => {
  const { cli, render } = compilerFixture(t)
  cli(['validate', PRODUCT])
  assert.deepEqual(render(), HISTORY)
})

test('symlink and nested directory rejected', t => {
  const { root, fragments, write, cli, commit, tag } = compilerFixture(t)
  const target = write('outside.md', '- Outside.\n')
  const path = join(fragments, 'patch-fixed-link.md')
  symlinkSync(target, path)
  cli(['validate', PRODUCT], false)
  commit()
  const release = tag('0.4.0.1')
  cli(['render', PRODUCT, '0.4.0.1', '--ref', release], false)
  unlinkSync(path)
  mkdirSync(path)
  cli(['validate', PRODUCT], false)
  rmdirSync(path)
  rmdirSync(fragments)
  symlinkSync(root, fragments, 'dir')
  cli(['validate', PRODUCT], false)
})

test('version injection rejected and prerelease accepted', t => {
  const { fragment, render, cli } = compilerFixture(t)
  fragment()
  render('0.4.0.1-rc.2+build.1')
  for (const version of ['1.2', '1.2.3\n## Injected', '[1.2.3]', '1.2.3/evil'])
    cli(['render', PRODUCT, version], false)
})

test('render reads tag not newer master', t => {
  const { fragment, commit, tag, render, history } = compilerFixture(t)
  const original = fragment(undefined, '- Tagged.\n')
  commit()
  const release = tag('0.4.0.1', true)
  const expected = render('0.4.0.1', release)
  writeFileSync(original, '- Modified on master.\n')
  fragment('minor-added-new.md', '- New on master.\n')
  writeFileSync(history, '# Completely different master history\n')
  commit()
  assert.deepEqual(render('0.4.0.1', release), expected)
  excludes(expected, 'master')
  contains(expected, '- Tagged.')
})

test('sync retains modified and new fragments and retries', t => {
  const { fragment, commit, tag, render, history, cli } = compilerFixture(t)
  const unchanged = fragment()
  const modified = fragment('minor-added-modified.md', '- Tagged added.\n')
  commit()
  const release = tag('0.4.0.1')
  const expected = render('0.4.0.1', release)
  writeFileSync(modified, '- Revised after tag.\n')
  const fresh = fragment('patch-fixed-new.md', '- New after tag.\n')
  commit()
  cli(['sync'])
  assert.deepEqual(readFileSync(history), expected)
  assert.ok(!existsSync(unchanged))
  assert.equal(readFileSync(modified, 'utf8'), '- Revised after tag.\n')
  assert.ok(existsSync(fresh))
  cli(['sync'])
  assert.deepEqual(readFileSync(history), expected)
  assert.ok(existsSync(modified))
  assert.ok(existsSync(fresh))
})

test('overlapping snapshots and pending ancestor sections', t => {
  const { fragment, commit, tag, render, history, fragments, cli } =
    compilerFixture(t)
  const first = fragment(undefined, '- First release.\n')
  commit()
  tag('0.4.0.1')
  fragment('minor-added-next.md', '- Second release.\n')
  commit()
  const release = tag('0.4.0.2')
  const expected = render('0.4.0.2', release)
  assert.equal(count(expected, '- First release.'), 1)
  assert.equal(count(expected, '- Second release.'), 1)
  assert.ok(expected.indexOf('## [0.4.0.2]') < expected.indexOf('## [0.4.0.1]'))
  cli(['sync'])
  assert.deepEqual(readFileSync(history), expected)
  assert.ok(!existsSync(first))
  assert.deepEqual(readdirSync(fragments), [])
  cli(['sync'])
  assert.deepEqual(readFileSync(history), expected)
})

test('topology not semver or commit time orders tags', t => {
  const { fragment, commit, tag, render, history, cli } = compilerFixture(t)
  fragment(undefined, '- Ancestor.\n')
  commit('2026-01-02T00:00:00Z')
  tag('0.9.0')
  fragment('patch-fixed-descendant.md', '- Descendant.\n')
  commit('2026-01-01T00:00:00Z')
  const release = tag('0.1.0')
  const expected = render('0.1.0', release)
  cli(['sync'])
  assert.deepEqual(readFileSync(history), expected)
  assert.ok(expected.indexOf('## [0.1.0]') < expected.indexOf('## [0.9.0]'))
})

test('unmerged tag excluded', t => {
  const { git, fragment, commit, tag, render, history, cli } =
    compilerFixture(t)
  git('checkout', '-qb', 'unmerged')
  fragment(undefined, '- Unmerged.\n')
  commit()
  tag('0.99.0')
  git('checkout', '-q', 'master')
  fragment(undefined, '- Merged.\n')
  commit()
  const release = tag('0.4.0.1')
  const rendered = render('0.4.0.1', release)
  cli(['sync'])
  assert.deepEqual(readFileSync(history), rendered)
  excludes(rendered, 'Unmerged')
  excludes(rendered, '0.99.0')
})

test('legacy tag and existing version heading are preserved', t => {
  const { tag, cli, history, render, fragment, commit } = compilerFixture(t)
  tag('0.3.0')
  cli(['sync'])
  assert.deepEqual(readFileSync(history), HISTORY)
  assert.deepEqual(render('0.3.0', 'start-os/v0.3.0'), HISTORY)
  const path = fragment()
  const existing = render()
  writeFileSync(history, existing)
  commit()
  const release = tag('0.4.0.1')
  assert.deepEqual(render('0.4.0.1', release), existing)
  cli(['sync'])
  assert.deepEqual(readFileSync(history), existing)
  assert.ok(!existsSync(path))
  cli(['sync'])
  assert.deepEqual(readFileSync(history), existing)
})

test('moved processed tag does not consume updated fragment', t => {
  const { fragment, commit, tag, cli, history, git } = compilerFixture(t)
  fragment()
  commit()
  tag('0.4.0.1')
  cli(['sync'])
  const previous = readFileSync(history)
  commit()
  const path = fragment(undefined, '- Replacement release text.\n')
  commit()
  git('tag', '-f', 'start-os/v0.4.0.1')
  cli(['sync'], false)
  cli(['render', PRODUCT, '0.4.0.1', '--ref', 'start-os/v0.4.0.1'], false)
  assert.deepEqual(readFileSync(history), previous)
  assert.ok(existsSync(path))
})

test('reintroduced identical fragment belongs to next release', t => {
  const { fragment, commit, tag, cli, render, history } = compilerFixture(t)
  fragment()
  commit()
  tag('0.4.0.1')
  cli(['sync'])
  commit()
  const path = fragment()
  cli(['check-version', PRODUCT, '0.4.0.2'])
  cli(['sync'])
  assert.ok(existsSync(path))
  commit()
  const release = tag('0.4.0.2')
  const rendered = render('0.4.0.2', release)
  assert.equal(count(rendered, '- Fixed example.'), 2)
  cli(['sync'])
  assert.deepEqual(readFileSync(history), rendered)
  assert.ok(!existsSync(path))
})

test('modified same path at next tag is new ownership', t => {
  const { fragment, commit, tag, render, cli, history } = compilerFixture(t)
  const path = fragment(undefined, '- Old text.\n')
  commit()
  tag('0.4.0.1')
  writeFileSync(path, '- New text.\n')
  commit()
  const release = tag('0.4.0.2')
  const rendered = render('0.4.0.2', release)
  assert.equal(count(rendered, '- Old text.'), 1)
  assert.equal(count(rendered, '- New text.'), 1)
  cli(['sync'])
  assert.deepEqual(readFileSync(history), rendered)
  assert.ok(!existsSync(path))
})

test('later sync reuses committed version markers', t => {
  const { fragment, commit, tag, cli, history, render } = compilerFixture(t)
  fragment(undefined, '- First release.\n')
  commit()
  tag('0.4.0.1')
  cli(['sync'])
  const firstHistory = readFileSync(history)
  commit()
  cli(['sync'])
  assert.deepEqual(readFileSync(history), firstHistory)
  fragment('minor-added-next.md', '- Second release.\n')
  commit()
  const release = tag('0.4.0.2')
  const expected = render('0.4.0.2', release)
  cli(['sync'])
  assert.deepEqual(readFileSync(history), expected)
  assert.equal(count(expected, '## [0.4.0.1]'), 1)
  const suffix = firstHistory.subarray(firstHistory.indexOf('## '))
  assert.deepEqual(expected.subarray(-suffix.length), suffix)
})

test('product ownership is independent', t => {
  const { fragment, write, commit, tag, git, cli, history, root } =
    compilerFixture(t)
  fragment(undefined, '- OS entry.\n')
  const other = write(
    'projects/start-sdk/changelog/patch-fixed-example.md',
    '- SDK entry.\n',
  )
  commit()
  tag('0.4.0.1')
  git('tag', 'start-sdk/v2.0.0')
  cli(['sync'])
  const osHistory = readFileSync(history)
  const sdkHistory = readFileSync(join(root, 'projects/start-sdk/CHANGELOG.md'))
  contains(osHistory, 'OS entry')
  excludes(osHistory, 'SDK entry')
  contains(sdkHistory, 'SDK entry')
  excludes(sdkHistory, 'OS entry')
  assert.ok(!existsSync(other))
})

test('version reads canonical manifests from absolute script', t => {
  const { manifest, cli } = compilerFixture(t)
  for (const name of [
    'start-os',
    'start-sdk',
    'start-cli',
    'start-tunnel',
    'start-registry',
    'start-wrt',
  ]) {
    const product = `projects/${name}`
    const version = name === 'start-os' ? '0.4.0.3' : '1.2.3'
    manifest(version, product)
    assert.equal(cli(['version', product]).stdout.toString().trim(), version)
  }
  cli(['version', 'projects/unknown'], false)
})

test('check version each tier and mismatching manifest', async t => {
  const { tag, fragment, manifest, cli } = compilerFixture(t)
  tag('0.4.0.3')
  for (const [tier, expected] of [
    ['patch', '0.4.0.4'],
    ['minor', '0.4.1'],
    ['major', '0.5.0'],
  ])
    await t.test(tier, () => {
      const path = fragment(`${tier}-changed-impact.md`)
      manifest(expected)
      cli(['check-version', PRODUCT])
      manifest('0.4.0.3')
      const result = cli(['check-version', PRODUCT], false)
      contains(result.stderr, expected)
      unlinkSync(path)
    })
})

test('changed unrelated PR after ancestor CLI release', t => {
  const { manifest, commit, write, git, cli } = compilerFixture(t)
  const product = 'projects/start-cli'
  manifest('2.3.0', product)
  const releaseCommit = commit()
  const path = write(
    `${product}/changelog/patch-fixed-pending.md`,
    '---\nuser-facing: false\n---\n\n- Pending CLI fix.\n',
  )
  const base = commit()
  git('tag', 'start-cli/v2.3.0', releaseCommit)
  write('README.md', 'Unrelated PR.\n')
  const unrelated = commit()
  assert.deepEqual(cli(['changed', base]).stdout, Buffer.from('\n'))
  contains(cli(['check-version', product], false).stderr, "expected '2.3.1'")
  writeFileSync(
    path,
    '---\nuser-facing: false\n---\n\n- Revised pending CLI fix.\n',
  )
  commit()
  assert.deepEqual(
    cli(['changed', unrelated]).stdout,
    Buffer.from('projects/start-cli\n'),
  )
  contains(cli(['check-version', product], false).stderr, "expected '2.3.1'")
  const fragmentHead = git('rev-parse', 'HEAD').trim()
  manifest('2.3.0+metadata', product)
  commit()
  assert.deepEqual(
    cli(['changed', fragmentHead]).stdout,
    Buffer.from('projects/start-cli\n'),
  )
  contains(cli(['check-version', product], false).stderr, "expected '2.3.1'")
})

test('changed uses merge base not base tip', t => {
  const { commit, git, manifest, write, cli, fragment } = compilerFixture(t)
  const base = commit()
  git('checkout', '-qb', 'base')
  manifest('1.2.3', 'projects/start-sdk')
  const baseTip = commit()
  git('checkout', '-q', 'master')
  write('README.md', 'PR-only change.\n')
  commit()
  assert.deepEqual(cli(['changed', baseTip]).stdout, Buffer.from('\n'))
  fragment()
  commit()
  assert.deepEqual(
    cli(['changed', baseTip]).stdout,
    Buffer.from('projects/start-os\n'),
  )
  assert.deepEqual(
    cli(['changed', base]).stdout,
    Buffer.from('projects/start-os\n'),
  )
})

test('changed deletion rename and multiple products', t => {
  const { fragment, commit, root, cli, git, manifest } = compilerFixture(t)
  const deleted = fragment('patch-fixed-deleted.md')
  const renamed = fragment('patch-fixed-renamed.md')
  const base = commit()
  unlinkSync(deleted)
  const destination = join(
    root,
    'projects/start-sdk/changelog/patch-fixed-renamed.md',
  )
  mkdirSync(join(root, 'projects/start-sdk/changelog'), { recursive: true })
  renameSync(renamed, destination)
  commit()
  assert.deepEqual(
    cli(['changed', base]).stdout,
    Buffer.from('projects/start-os projects/start-sdk\n'),
  )
  const deletionBase = git('rev-parse', 'HEAD').trim()
  unlinkSync(destination)
  commit()
  assert.deepEqual(
    cli(['changed', deletionBase]).stdout,
    Buffer.from('projects/start-sdk\n'),
  )
  manifest('1.2.3', 'projects/start-cli')
  const manifestBase = commit()
  renameSync(
    join(root, 'projects/start-cli/Cargo.toml'),
    join(root, 'removed-manifest.toml'),
  )
  commit()
  assert.deepEqual(
    cli(['changed', manifestBase]).stdout,
    Buffer.from('projects/start-cli\n'),
  )
})

test('changed selects changelog directory replacements', t => {
  const { git, fragments, root, commit, cli } = compilerFixture(t)
  const base = git('rev-parse', 'HEAD').trim()
  rmdirSync(fragments)
  symlinkSync(root, fragments, 'dir')
  commit()
  assert.deepEqual(
    cli(['changed', base]).stdout,
    Buffer.from('projects/start-os\n'),
  )
  unlinkSync(fragments)
  writeFileSync(fragments, 'Not a directory.\n')
  const replacementBase = git('rev-parse', 'HEAD').trim()
  commit()
  assert.deepEqual(
    cli(['changed', replacementBase]).stdout,
    Buffer.from('projects/start-os\n'),
  )
  const deletionBase = git('rev-parse', 'HEAD').trim()
  unlinkSync(fragments)
  commit()
  assert.deepEqual(
    cli(['changed', deletionBase]).stdout,
    Buffer.from('projects/start-os\n'),
  )
})

test('changed matches canonical manifest paths exactly', async t => {
  const { git, manifest, commit, cli, write } = compilerFixture(t)
  for (const product of [
    'projects/start-os',
    'projects/start-sdk',
    'projects/start-cli',
    'projects/start-tunnel',
    'projects/start-registry',
    'projects/start-wrt',
  ])
    await t.test(product, () => {
      const base = git('rev-parse', 'HEAD').trim()
      manifest(product === PRODUCT ? '0.4.0.3' : '1.2.3', product)
      commit()
      assert.deepEqual(
        cli(['changed', base]).stdout,
        Buffer.from(`${product}\n`),
      )
    })
  const base = git('rev-parse', 'HEAD').trim()
  for (const name of [
    'projects/start-os/Cargo.toml',
    'projects/start-os/package.json',
    'projects/start-wrt/Cargo.toml',
    'projects/start-sdk/package.json.backup',
    'projects/start-cli/changelog-backup/patch-fixed-example.md',
  ])
    write(name, 'Unrelated path.\n')
  commit()
  assert.deepEqual(cli(['changed', base]).stdout, Buffer.from('\n'))
})

test('changed handles NUL delimited paths', t => {
  const { git, fragment, commit, cli } = compilerFixture(t)
  const base = git('rev-parse', 'HEAD').trim()
  fragment('patch-fixed-name\nwith-newline.md')
  commit()
  assert.deepEqual(
    cli(['changed', base]).stdout,
    Buffer.from('projects/start-os\n'),
  )
  cli(['changed', 'nonexistent-base'], false)
})

test('projects preserves release caller contract', t => {
  const { cli } = compilerFixture(t)
  assert.deepEqual(
    cli(['projects']).stdout,
    Buffer.from(
      'start-os start-sdk start-cli start-tunnel start-registry start-wrt\n',
    ),
  )
})

test('mixed tiers use highest and omit StartOS zero revision', t => {
  const { tag, fragment, manifest, cli } = compilerFixture(t)
  tag('0.4.0.3')
  fragment('patch-fixed-small.md')
  fragment('minor-added-medium.md')
  manifest('0.4.1')
  cli(['check-version', PRODUCT])
  cli(['check-version', PRODUCT, '0.4.1.0'], false)
  fragment('major-removed-large.md')
  manifest('0.5.0')
  cli(['check-version', PRODUCT])
  cli(['check-version', PRODUCT, '0.4.1'], false)
})

test('check version validates malformed fragments even without baseline', async t => {
  const { manifest, fragment, cli } = compilerFixture(t)
  manifest('0.4.0.3')
  for (const [name, body] of [
    ['fixed-invalid.md', '- Invalid name.\n'],
    ['patch-fixed-invalid.md', '## Invalid heading\n'],
  ])
    await t.test(name, () => {
      const path = fragment(name, body)
      cli(['check-version', PRODUCT], false)
      unlinkSync(path)
    })
  fragment('major-added-first-release.md')
  cli(['check-version', PRODUCT])
  cli(['check-version', PRODUCT, '0.9.0'])
  cli(['check-version', PRODUCT, '1.0.0'], false)
})

test('no fragments keeps latest release and override is caller data', t => {
  const { tag, manifest, cli, root } = compilerFixture(t)
  tag('0.4.0.3')
  manifest('0.4.0.3')
  cli(['check-version', PRODUCT])
  manifest('0.4.0.4')
  cli(['check-version', PRODUCT], false)
  cli(['check-version', PRODUCT, '0.4.0.3'])
  unlinkSync(join(root, 'package.json'))
  cli(['check-version', PRODUCT, '0.4.0.3'])
  cli(['check-version', PRODUCT], false)
})

test('lingering released tiers are not counted again', t => {
  const { fragment, commit, tag, manifest, cli } = compilerFixture(t)
  fragment('major-added-released.md', '- Already released.\n')
  commit()
  tag('0.4.0.3', true)
  manifest('0.4.0.3')
  cli(['check-version', PRODUCT])
  fragment('patch-fixed-pending.md')
  manifest('0.4.0.4')
  cli(['check-version', PRODUCT])
  cli(['check-version', PRODUCT, '0.5.0'], false)
})

test('modified path is pending but identical ancestor path is not', t => {
  const { fragment, commit, tag, manifest, cli } = compilerFixture(t)
  const path = fragment('minor-added-feature.md', '- Released feature.\n')
  commit()
  tag('0.4.0.3')
  fragment('minor-added-feature.md', '- Revised feature.\n')
  manifest('0.4.1')
  cli(['check-version', PRODUCT])
  cli(['check-version', PRODUCT, '0.4.0.3'], false)
})

test('live origin baseline is fresh and local unpublished tags ignored', t => {
  const { root, git, tag, fragment, manifest, cli } = compilerFixture(t)
  const remote = join(root, 'remote.git')
  git('init', '--bare', '-q', remote)
  git('remote', 'set-url', 'origin', remote)
  tag('0.4.0.3')
  git('push', '-q', 'origin', 'start-os/v0.4.0.3')
  fragment()
  manifest('0.4.0.4')
  tag('0.9.0')
  cli(['check-version', PRODUCT])
  tag('0.4.0.8')
  git('push', '-q', 'origin', 'start-os/v0.4.0.8')
  cli(['check-version', PRODUCT], false)
  manifest('0.4.0.9')
  cli(['check-version', PRODUCT])
})

test('latest stable is numeric not topological and prereleases ignored', t => {
  const { tag, commit, fragment, manifest, cli } = compilerFixture(t)
  tag('0.4.0.10')
  commit()
  tag('0.4.0.9')
  tag('0.5.0-rc.1')
  fragment()
  manifest('0.4.0.11')
  cli(['check-version', PRODUCT])
})

test('check version queries origin once', t => {
  const { tag, manifest, root, env, cli } = compilerFixture(t)
  tag('0.4.0.3')
  manifest('0.4.0.3')
  const trace = join(root, 'git.trace')
  env.GIT_TRACE = trace
  cli(['check-version', PRODUCT])
  assert.equal(
    count(readFileSync(trace), 'built-in: git ls-remote --tags origin'),
    1,
  )
})

test('unknown origin tag namespace is ignored', t => {
  const { git, tag, manifest, cli } = compilerFixture(t)
  git('tag', 'unrelated/vnot-a-version')
  tag('0.4.0.3')
  manifest('0.4.0.3')
  cli(['check-version', PRODUCT])
  cli(['sync'])
})

test('nonancestor release is baseline but does not own pending fragments', t => {
  const { git, fragment, commit, tag, manifest, cli } = compilerFixture(t)
  git('checkout', '-qb', 'unmerged')
  fragment()
  commit()
  tag('0.4.0.3')
  git('checkout', '-q', 'master')
  fragment()
  manifest('0.4.0.4')
  cli(['check-version', PRODUCT])
})

test('check version semver product', t => {
  const { git, write, manifest, cli } = compilerFixture(t)
  const product = 'projects/start-sdk'
  git('tag', 'start-sdk/v2.0.9')
  write(
    `${product}/changelog/minor-added-feature.md`,
    '---\nuser-facing: false\n---\n\n- Feature.\n',
  )
  manifest('2.1.0', product)
  cli(['check-version', product])
  cli(['check-version', product, '2.0.10'], false)
})

test('fragment audience is mandatory and boolean', t => {
  const { fragment, cli } = compilerFixture(t)
  for (const body of [
    '- Missing metadata.\n',
    '---\nuser-facing: yes\n---\n\n- Invalid.\n',
    '---\nuser-facing: "false"\n---\n\n- Invalid.\n',
    '---\nuser-facing: true\nextra: value\n---\n\n- Invalid.\n',
  ]) {
    fragment(undefined, body, null)
    contains(
      cli(['validate', PRODUCT], false).stderr,
      'user-facing: true or false',
    )
  }
  for (const facing of [true, false]) {
    fragment(undefined, '- Valid.\n', facing)
    cli(['validate', PRODUCT])
  }
})

test('highlights share fragment bodies and preserve release framing', t => {
  const { fragment, cli, render, write } = compilerFixture(t)
  fragment(
    'patch-fixed-z.md',
    '- Visible fix.\n  With details.\n\n- Another fix.\n',
  )
  fragment('minor-added-a.md', '- Visible feature.\n')
  fragment('patch-changed-internal.md', '- Internal change.\n', false)
  write(
    `${PRODUCT}/release-notes/0.4.0.1.md`,
    'Lede.\n\n## Important\n\nKeep this notice.\n',
  )
  const notes = cli(['notes', PRODUCT, '0.4.0.1']).stdout
  assert.equal(
    notes.toString(),
    'Lede.\n\n## Highlights\n\n- Visible feature.\n\n- Visible fix.\n  With details.\n\n- Another fix.\n\n## Important\n\nKeep this notice.\n',
  )
  contains(render(), 'Internal change.')
  excludes(notes, 'Internal change.')
  excludes(notes, 'user-facing:')
  excludes(render(), 'user-facing:')
})

test('notes compile without framing or an origin connection', t => {
  const { fragment, cli, git } = compilerFixture(t)
  git('remote', 'set-url', 'origin', '/unavailable-origin')
  assert.equal(
    cli(['notes', PRODUCT, '0.4.0.1']).stdout.toString(),
    '## Highlights\n',
  )
  fragment(undefined, '- Highlight.\n')
  assert.equal(
    cli(['notes', PRODUCT, '0.4.0.1']).stdout.toString(),
    '## Highlights\n\n- Highlight.\n',
  )
  fragment(undefined, '- Internal.\n', false)
  assert.equal(
    cli(['notes', PRODUCT, '0.4.0.1']).stdout.toString(),
    '## Highlights\n',
  )
})

test('sync archives highlights before consuming tagged fragments', t => {
  const { root, fragment, write, cli, commit, tag, git } = compilerFixture(t)
  const path = `${PRODUCT}/release-notes/0.4.0.1.md`
  const framing = 'Lede.\n\n## Important\n\nImportant notice.\n'
  write(path, framing)
  const consumed = fragment(undefined, '- Tagged highlight.\n')
  fragment('patch-changed-internal.md', '- Internal.\n', false)
  commit()
  const release = tag('0.4.0.1', true)
  const expected = cli(['notes', PRODUCT, '0.4.0.1', '--ref', release]).stdout
  fragment('minor-added-new.md', '- Future highlight.\n')
  commit()
  cli(['sync'])
  assert.equal(existsSync(consumed), false)
  assert.deepEqual(readFileSync(join(root, path)), expected)
  excludes(expected, 'Future highlight.')
  excludes(expected, 'Internal.')
  cli(['sync'])
  assert.deepEqual(readFileSync(join(root, path)), expected)
  commit()
  assert.deepEqual(
    cli(['notes', PRODUCT, '0.4.0.1', '--ref', release]).stdout,
    expected,
  )
  assert.equal(git('show', `${release}:${path}`), framing)
  const upcoming = cli(['notes', PRODUCT, '0.4.0.2']).stdout
  contains(upcoming, 'Future highlight.')
  excludes(upcoming, 'Tagged highlight.')
})

test('notes ownership excludes lingering ancestor fragments and preserves legacy releases', t => {
  const { root, fragment, write, cli, commit, tag } = compilerFixture(t)
  const legacy =
    'Legacy lede.\r\n\r\n## Highlights\r\n\r\n- Curated legacy highlight.\r\n'
  const legacyPath = `${PRODUCT}/release-notes/0.4.0.md`
  write(legacyPath, legacy)
  fragment(undefined, '- Legacy changelog.\n', null)
  commit()
  const old = tag('0.4.0')
  fragment('patch-added-first.md', '- First highlight.\n')
  commit()
  const first = tag('0.4.0.1', true)
  fragment('patch-fixed-second.md', '- Second highlight.\n')
  commit()
  const second = tag('0.4.0.2')
  const notes = cli(['notes', PRODUCT, '0.4.0.2', '--ref', second]).stdout
  contains(notes, 'Second highlight.')
  excludes(notes, 'First highlight.')
  excludes(notes, 'Legacy changelog.')
  cli(['sync'])
  assert.equal(readFileSync(join(root, legacyPath), 'utf8'), legacy)
  assert.equal(
    cli(['notes', PRODUCT, '0.4.0', '--ref', old]).stdout.toString(),
    legacy,
  )
  assert.equal(
    cli(['notes', PRODUCT, '0.4.0.1', '--ref', first]).stdout.toString(),
    '## Highlights\n\n- First highlight.\n',
  )
  assert.equal(
    readFileSync(join(root, `${PRODUCT}/release-notes/0.4.0.2.md`), 'utf8'),
    '## Highlights\n\n- Second highlight.\n',
  )
  assert.equal(
    cli(['notes', PRODUCT, '0.4.0.2']).stdout.toString(),
    '## Highlights\n\n- Second highlight.\n',
  )
})

test('audience-only tag changes cannot rewrite archived highlights', t => {
  const { root, fragment, cli, commit, tag, git } = compilerFixture(t)
  fragment(undefined, '- Released highlight.\n')
  commit()
  tag('0.4.0.1')
  cli(['sync'])
  const path = join(root, PRODUCT, 'release-notes/0.4.0.1.md')
  const archived = readFileSync(path)
  commit()
  fragment(undefined, '- Released highlight.\n', false)
  commit()
  git('tag', '-f', 'start-os/v0.4.0.1')
  contains(cli(['sync'], false).stderr, 'remove ## Highlights')
  assert.deepEqual(readFileSync(path), archived)
})

test('security fragments form their own section after Highlights and before notices', t => {
  const { fragment, write, cli, render } = compilerFixture(t)
  fragment('minor-added-feature.md', '- Visible feature.\n')
  fragment('patch-security-a.md', '- Security fix.\n')
  fragment('patch-security-b.md', '- Security fallback.\n')
  fragment('patch-security-internal.md', '- Internal security detail.\n', false)
  write(
    `${PRODUCT}/changelog/patch-security-a.pl_PL.md`,
    '- Poprawka bezpieczeństwa.\n',
  )
  write(
    `${PRODUCT}/release-notes/0.4.0.1.md`,
    '## Important\n\nKeep this notice.\n',
  )
  const english = cli(['notes', PRODUCT, '0.4.0.1']).stdout.toString()
  assert.equal(
    english,
    '## Highlights\n\n- Visible feature.\n\n## Security\n\n- Security fix.\n\n- Security fallback.\n\n## Important\n\nKeep this notice.\n',
  )
  const polish = cli(['notes', PRODUCT, '0.4.0.1', '--locale', 'pl_PL']).stdout
  contains(polish, '## Nowości\n\n- Visible feature.')
  contains(polish, '## Bezpieczeństwo\n\n- Poprawka bezpieczeństwa.')
  contains(polish, '- Security fallback.')
  contains(polish, '## Important\n\nKeep this notice.')
  excludes(polish, 'Internal security detail.')
  const history = render()
  contains(history, '### Security')
  contains(history, 'Internal security detail.')
})

test('security-only releases need no framing and use catalog headings in every locale', t => {
  const { root, fragment, cli } = compilerFixture(t)
  fragment('patch-security-fix.md', '- Security fix.\n')
  for (const [locale, highlights, security] of [
    ['en_US', 'Highlights', 'Security'],
    ['de_DE', 'Neuerungen', 'Sicherheit'],
    ['es_ES', 'Novedades', 'Seguridad'],
    ['fr_FR', 'Nouveautés', 'Sécurité'],
    ['pl_PL', 'Nowości', 'Bezpieczeństwo'],
  ]) {
    assert.equal(
      cli(['notes', PRODUCT, '0.4.0.1', '--locale', locale]).stdout.toString(),
      `## ${highlights}\n\n## ${security}\n\n- Security fix.\n`,
    )
  }
  assert.ok(!existsSync(join(root, PRODUCT, 'release-notes')))
  fragment('patch-security-fix.md', '- Internal.\n', false)
  assert.equal(
    cli(['notes', PRODUCT, '0.4.0.1']).stdout.toString(),
    '## Highlights\n',
  )
})

test('security archives preserve translated sections and pinned titles without framing', t => {
  const { root, fragment, write, cli, commit, tag, git } = compilerFixture(t)
  fragment('patch-security-fix.md', '- Security fix.\n')
  write(`${PRODUCT}/changelog/patch-security-fix.pl_PL.md`, '- Bezpieczniej.\n')
  const catalog = 'shared-libs/crates/start-core/locales/release-notes.json'
  const labels = JSON.parse(readFileSync(join(REPO, catalog), 'utf8'))
  labels['release-notes.security-title'].pl_PL = 'Zapisane bezpieczeństwo'
  write(catalog, JSON.stringify(labels))
  commit()
  const release = tag('0.4.0.1')
  const args = [
    'notes',
    PRODUCT,
    '0.4.0.1',
    '--locale',
    'pl_PL',
    '--ref',
    release,
  ]
  const expected = cli(args).stdout
  contains(expected, '## Zapisane bezpieczeństwo')
  cli(['sync'])
  assert.deepEqual(
    readFileSync(join(root, PRODUCT, 'release-notes/0.4.0.1.pl_PL.md')),
    expected,
  )
  assert.ok(!existsSync(join(root, PRODUCT, 'changelog/patch-security-fix.md')))
  commit()
  cli(['sync'])
  assert.deepEqual(cli(args).stdout, expected)
  assert.ok(
    cli(['archives', PRODUCT]).stdout.toString().includes('0.4.0.1.pl_PL.md'),
  )
  assert.equal(
    git(
      'ls-tree',
      '-r',
      '--name-only',
      release,
      '--',
      `${PRODUCT}/release-notes`,
    ),
    '',
  )
})

test('authored Security sections are rejected before replacing generated notes', t => {
  const { fragment, write, cli } = compilerFixture(t)
  fragment('patch-security-fix.md', '- Security fix.\n')
  for (const [locale, heading] of [
    ['en_US', 'Security'],
    ['pl_PL', 'Bezpieczeństwo'],
  ]) {
    const suffix = locale === 'en_US' ? '' : `.${locale}`
    write(
      `${PRODUCT}/release-notes/0.4.0.1${suffix}.md`,
      `## ${heading}\n\n- Hand-written.\n`,
    )
    contains(
      cli(['notes', PRODUCT, '0.4.0.1', '--locale', locale], false).stderr,
      `remove ## ${heading}`,
    )
  }
})

test('translated Highlights inherit audience, ordering, and English fallback', t => {
  const { fragment, write, cli, render } = compilerFixture(t)
  fragment('minor-added-a.md', '- English feature.\n')
  write(`${PRODUCT}/changelog/minor-added-a.pl_PL.md`, '- Polska funkcja.\n')
  fragment('patch-fixed-b.md', '- English fallback.\n')
  fragment('patch-changed-internal.md', '- Internal.\n', false)
  write(
    `${PRODUCT}/changelog/patch-changed-internal.pl_PL.md`,
    '- Wewnętrzne.\n',
  )
  write(
    `${PRODUCT}/release-notes/0.4.0.1.pl_PL.md`,
    'Wstęp.\n\n## Nowości\n\n<!-- highlights -->\n\n## Ważne\n\nInstrukcje.\n',
  )
  const notes = cli(['notes', PRODUCT, '0.4.0.1', '--locale', 'pl_PL']).stdout
  assert.equal(
    notes.toString(),
    'Wstęp.\n\n## Nowości\n\n- Polska funkcja.\n\n- English fallback.\n\n## Ważne\n\nInstrukcje.\n',
  )
  excludes(notes, 'Internal.')
  excludes(notes, 'Wewnętrzne.')
  excludes(notes, '<!-- highlights -->')
  const history = render('0.4.1')
  contains(history, 'English feature.')
  contains(history, 'Internal.')
  excludes(history, 'Polska funkcja.')
  const bundle = JSON.parse(cli(['notes-bundle', PRODUCT, '0.4.0.1']).stdout)
  assert.deepEqual(Object.keys(bundle), ['en_US', 'pl_PL'])
  assert.equal(bundle.pl_PL, notes.toString())
  contains(bundle.en_US, 'English feature.')
  assert.equal(
    cli(['locales', PRODUCT, '0.4.0.1']).stdout.toString(),
    'pl_PL\n',
  )
  cli(['notes', PRODUCT, '0.4.0.1', '--locale', '../pl_PL'], false)
  cli(['notes', PRODUCT, '0.4.0.1', '--locale', 'pl_PL\n'], false)
})

test('translation companions require canonical bodies and valid bullet Markdown', t => {
  const { fragment, write, cli } = compilerFixture(t)
  const path = write(
    `${PRODUCT}/changelog/patch-fixed-example.pl_PL.md`,
    '- Polska poprawka.\n',
  )
  contains(
    cli(['validate', PRODUCT], false).stderr,
    'missing canonical fragment',
  )
  fragment()
  cli(['validate', PRODUCT])
  const malformedName = write(
    `${PRODUCT}/changelog/patch-fixed-example.pl_PL.md\n`,
    '- Poprawka.\n',
  )
  cli(['validate', PRODUCT], false)
  unlinkSync(malformedName)
  writeFileSync(path, '## Nowości\n')
  contains(cli(['validate', PRODUCT], false).stderr, 'headings are not allowed')
  writeFileSync(path, '---\nuser-facing: false\n---\n\n- Wewnętrzne.\n')
  cli(['validate', PRODUCT], false)
  unlinkSync(path)
  write(
    `${PRODUCT}/changelog/patch-fixed-example.en_US.md`,
    '- English duplicate.\n',
  )
  contains(
    cli(['validate', PRODUCT], false).stderr,
    'English belongs in the canonical fragment',
  )
})

test('locale discovery includes framing and visible companions without a formatter', t => {
  const { root, fragment, write, cli } = compilerFixture(t)
  fragment()
  write(`${PRODUCT}/changelog/patch-fixed-example.pl_PL.md`, '- Poprawka.\n')
  fragment('patch-changed-internal.md', '- Internal.\n', false)
  write(`${PRODUCT}/changelog/patch-changed-internal.es_ES.md`, '- Interno.\n')
  write(
    `${PRODUCT}/release-notes/0.4.0.1.de_DE.md`,
    '## Neuigkeiten\n\n<!-- highlights -->\n',
  )
  write(`${PRODUCT}/release-notes/0.4.0.2.fr_FR.md`, 'Future.\n')
  write(`${PRODUCT}/release-notes/0.4.0.1.pre-update.md`, 'Warning.\n')
  unlinkSync(join(root, '.prettierrc.json'))
  assert.equal(
    cli(['locales', PRODUCT, '0.4.0.1']).stdout.toString(),
    'de_DE pl_PL\n',
  )
})

test('localized framing reserves an empty generated section', t => {
  const { fragment, write, cli } = compilerFixture(t)
  fragment()
  const path = `${PRODUCT}/release-notes/0.4.0.1.pl_PL.md`
  for (const text of [
    '## Nowości\n\n- Ręcznie.\n',
    '## Nowości\n\n- Ręcznie.\n\n<!-- highlights -->\n',
    '## Nowości\n\n<!-- highlights -->\n\n- Ręcznie.\n',
    '<!-- highlights -->\n',
    '## Nowości\n\n<!-- highlights -->\n\n<!-- highlights -->\n',
  ]) {
    write(path, text)
    cli(['notes', PRODUCT, '0.4.0.1', '--locale', 'pl_PL'], false)
    cli(['notes-bundle', PRODUCT, '0.4.0.1'], false)
  }
  fragment(undefined, '- Internal.\n', false)
  write(path, 'Wstęp.\n\n## Nowości\n\n<!-- highlights -->\n')
  assert.equal(
    cli(['notes', PRODUCT, '0.4.0.1', '--locale', 'pl_PL']).stdout.toString(),
    'Wstęp.\n\n## Nowości\n',
  )
})

test('sync archives translations from the tag and consumes unchanged bundles', t => {
  const { root, fragment, write, cli, commit, tag, git } = compilerFixture(t)
  const canonical = fragment()
  const translation = write(
    `${PRODUCT}/changelog/patch-fixed-example.pl_PL.md`,
    '- Pierwsza poprawka.\n',
  )
  const framing = `${PRODUCT}/release-notes/0.4.0.1.pl_PL.md`
  write(framing, 'Wstęp.\n\n## Nowości\n\n<!-- highlights -->\n')
  commit()
  const release = tag('0.4.0.1', true)
  const expected = cli([
    'notes',
    PRODUCT,
    '0.4.0.1',
    '--locale',
    'pl_PL',
    '--ref',
    release,
  ]).stdout
  writeFileSync(translation, '- Zmieniona poprawka.\n')
  const added = write(
    `${PRODUCT}/changelog/patch-fixed-example.de_DE.md`,
    '- Erste Korrektur.\n',
  )
  const unchanged = fragment('patch-fixed-other.md', '- Another English fix.\n')
  const unchangedTranslation = write(
    `${PRODUCT}/changelog/patch-fixed-other.pl_PL.md`,
    '- Inna poprawka.\n',
  )
  commit()
  tag('0.4.0.2')
  cli(['sync'])
  assert.deepEqual(readFileSync(join(root, framing)), expected)
  assert.ok(!existsSync(canonical))
  assert.ok(!existsSync(translation))
  assert.ok(!existsSync(added))
  assert.ok(!existsSync(unchanged))
  assert.ok(!existsSync(unchangedTranslation))
  assert.equal(
    readFileSync(
      join(root, `${PRODUCT}/release-notes/0.4.0.2.pl_PL.md`),
      'utf8',
    ),
    '## Nowości\n\n- Zmieniona poprawka.\n\n- Inna poprawka.\n',
  )
  assert.equal(
    readFileSync(
      join(root, `${PRODUCT}/release-notes/0.4.0.2.de_DE.md`),
      'utf8',
    ),
    '## Neuerungen\n\n- Erste Korrektur.\n\n- Another English fix.\n',
  )
  commit()
  cli(['sync'])
  assert.deepEqual(
    cli(['notes', PRODUCT, '0.4.0.1', '--locale', 'pl_PL', '--ref', release])
      .stdout,
    expected,
  )
  assert.deepEqual(readFileSync(join(root, framing)), expected)
  write(`${PRODUCT}/release-notes/0.4.0.3.fr_FR.md`, 'Unreleased.\n')
  commit()
  const archives = cli(['archives', PRODUCT])
    .stdout.toString()
    .trim()
    .split('\n')
  assert.ok(archives.includes(`${PRODUCT}/release-notes/0.4.0.1.pl_PL.md`))
  assert.ok(archives.includes(`${PRODUCT}/release-notes/0.4.0.2.de_DE.md`))
  assert.ok(!archives.includes(`${PRODUCT}/release-notes/0.4.0.3.fr_FR.md`))
  assert.equal(
    git('show', `${release}:${framing}`),
    'Wstęp.\n\n## Nowości\n\n<!-- highlights -->\n',
  )
})

test('post-tag translation edits and additions retain the canonical fragment', t => {
  const { fragment, write, cli, commit, tag } = compilerFixture(t)
  const canonical = fragment()
  const translation = write(
    `${PRODUCT}/changelog/patch-fixed-example.pl_PL.md`,
    '- Poprawka.\n',
  )
  commit()
  tag('0.4.0.1')
  writeFileSync(translation, '- Zmieniona.\n')
  const fresh = write(
    `${PRODUCT}/changelog/patch-fixed-example.de_DE.md`,
    '- Korrektur.\n',
  )
  commit()
  cli(['sync'])
  assert.ok(existsSync(canonical))
  assert.ok(existsSync(translation))
  assert.ok(existsSync(fresh))
  cli(['validate', PRODUCT])
  cli(['check-version', PRODUCT, '0.4.0.2'])
  cli(['sync'])
  assert.ok(existsSync(canonical))
})

test('deleted and reintroduced translations create new fragment ownership', t => {
  const { fragment, write, commit, tag, cli, render } = compilerFixture(t)
  fragment()
  const translation = write(
    `${PRODUCT}/changelog/patch-fixed-example.pl_PL.md`,
    '- Poprawka.\n',
  )
  commit()
  tag('0.4.0.1')
  unlinkSync(translation)
  commit()
  writeFileSync(translation, '- Poprawka.\n')
  commit()
  const release = tag('0.4.0.2')
  assert.equal(count(render('0.4.0.2', release), '- Fixed example.'), 2)
  cli(['sync'])
  assert.ok(!existsSync(translation))
})

test('legacy translated note bytes are preserved', t => {
  const { root, fragment, write, commit, tag, cli } = compilerFixture(t)
  const path = `${PRODUCT}/release-notes/0.4.0.1.pl_PL.md`
  const legacy = 'Wstęp.\r\n\r\n## Nowości\r\n\r\n- Opublikowane.\r\n'
  write(path, legacy)
  fragment(undefined, '- Legacy change.\n', null)
  commit()
  const release = tag('0.4.0.1')
  assert.equal(
    cli([
      'notes',
      PRODUCT,
      '0.4.0.1',
      '--locale',
      'pl_PL',
      '--ref',
      release,
    ]).stdout.toString(),
    legacy,
  )
  cli(['sync'])
  assert.equal(readFileSync(join(root, path), 'utf8'), legacy)
})

test('notes reject authored highlights before release', t => {
  const { fragment, write, cli } = compilerFixture(t)
  fragment(undefined, '- Compiled.\n')
  for (const framing of [
    'Lede.\n\n## Highlights\n\n- Hand-written.\n',
    'Lede.\n\n## Highlights\n\n- Hand-written.\n\n## Important\n\n<!-- highlights -->\n',
  ]) {
    write(`${PRODUCT}/release-notes/0.4.0.1.md`, framing)
    contains(
      cli(['notes', PRODUCT, '0.4.0.1'], false).stderr,
      'release-notes/0.4.0.1.md: remove ## Highlights',
    )
  }
})

test('notes and fragment consumption are atomic on framing conflicts', t => {
  const { fragment, write, cli, commit, tag } = compilerFixture(t)
  const path = `${PRODUCT}/release-notes/0.4.0.1.md`
  write(path, 'Tagged lede.\n')
  const pending = fragment()
  commit()
  tag('0.4.0.1')
  write(path, 'Edited after tag.\n')
  contains(cli(['sync'], false).stderr, 'release notes differ')
  assert.ok(existsSync(pending))
})

test('consumption does not follow replacement symlink', t => {
  const { fragment, commit, tag, write, cli } = compilerFixture(t)
  const path = fragment()
  commit()
  tag('0.4.0.1')
  const outside = write('outside.md', readFileSync(path))
  unlinkSync(path)
  symlinkSync(outside, path)
  cli(['sync'])
  assert.ok(lstatSync(path).isSymbolicLink())
  assert.ok(existsSync(outside))
})

test('security metadata follows fragment ownership and includes non-highlighted changes', t => {
  const { fragment, commit, tag, cli } = compilerFixture(t)
  const security = (version, ref) =>
    cli(['security', PRODUCT, version, ...(ref ? ['--ref', ref] : [])])
      .stdout.toString()
      .trim()
  assert.equal(security('0.4.0'), 'false')
  fragment('patch-security-private.md', '- Security correction.\n', false)
  commit()
  tag('0.4.0')
  assert.equal(security('0.4.0', 'start-os/v0.4.0'), 'true')
  assert.equal(security('0.4.0.1'), 'false')
  fragment(
    'patch-security-private.md',
    '- Another security correction.\n',
    false,
  )
  const updated = commit()
  assert.equal(security('0.4.0.1'), 'true')
  assert.equal(security('0.4.0.1', updated), 'true')
  assert.equal(security('0.4.0.1', 'start-os/v0.4.0'), 'false')
})

test('StartOS RC tags leave fragments for subsequent candidates and stable release', t => {
  const { root, fragment, commit, tag, cli, render } = compilerFixture(t)
  fragment('patch-security-example.md', '- Security correction.\n')
  const ref = commit()
  tag('0.4.0-rc.1')
  contains(render('0.4.0', ref), '- Security correction.')
  contains(
    cli(['notes', PRODUCT, '0.4.0', '--ref', ref]).stdout,
    '- Security correction.',
  )
  assert.equal(
    cli(['security', PRODUCT, '0.4.0', '--ref', ref]).stdout.toString(),
    'true\n',
  )
  cli(['sync'])
  assert.ok(
    existsSync(join(root, PRODUCT, 'changelog/patch-security-example.md')),
  )
  tag('0.4.0')
  cli(['sync'])
  assert.equal(
    existsSync(join(root, PRODUCT, 'changelog/patch-security-example.md')),
    false,
  )
})
