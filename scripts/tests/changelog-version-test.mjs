import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { test } from 'node:test'
import {
  MANIFESTS,
  latestRelease,
  manifestVersion,
  nextVersion,
  parseVersion,
} from '../changelog-version.mjs'

const PROJECTS = [
  'start-os',
  'start-cli',
  'start-sdk',
  'start-tunnel',
  'start-registry',
  'start-wrt',
]
const TIERS = ['patch', 'minor', 'major']

function* combinations(values, length) {
  if (!length) yield []
  else
    for (const value of values)
      for (const rest of combinations(values, length - 1))
        yield [value, ...rest]
}

function* permutations(values) {
  if (!values.length) yield []
  else
    for (let i = 0; i < values.length; i++)
      for (const rest of permutations(values.filter((_, index) => index !== i)))
        yield [values[i], ...rest]
}

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'changelog-version-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  return {
    root,
    write(path, text) {
      const target = join(root, path)
      mkdirSync(dirname(target), { recursive: true })
      writeFileSync(target, text)
    },
  }
}

test('public parser normalizes StartOS and preserves prerelease', () => {
  assert.deepEqual(parseVersion('start-os', '0.4.1'), [[4n, 1n, 0n], null])
  assert.deepEqual(parseVersion('start-os', '0.4.0.3-rc.1'), [
    [4n, 0n, 3n],
    'rc.1',
  ])
  assert.deepEqual(parseVersion('start-sdk', '4.0.3+build.1'), [
    [4n, 0n, 3n],
    null,
  ])
})
test('public parser rejects noncanonical versions', () => {
  for (const [product, version] of [
    ['start-os', '9.0.0'],
    ['start-sdk', '1.2.3.4'],
    ['start-sdk', '1.2.3-rc.01'],
    ['other', '1.2.3'],
  ])
    assert.throws(() => parseVersion(product, version))
})
test('all tier combinations and orders', () => {
  for (const [project, baseline, results] of [
    ['start-os', '0.4.0.3', ['0.4.0.4', '0.4.1', '0.5.0']],
    ['start-sdk', '4.0.3', ['4.0.4', '4.1.0', '5.0.0']],
  ]) {
    for (let length = 1; length < 5; length++)
      for (const tiers of combinations(TIERS, length)) {
        assert.equal(
          nextVersion(project, baseline, tiers.values()),
          results[Math.max(...tiers.map(tier => TIERS.indexOf(tier)))],
        )
      }
  }
})
test('empty fragments preserve exact version', () => {
  for (const project of PROJECTS)
    for (const version of project === 'start-os'
      ? ['0.4.0', '0.4.0.0', '0.4.0.3']
      : ['0.0.0', '1.2.3'])
      assert.equal(nextVersion(project, version, [].values()), version)
})
test('carry and reset boundaries', () => {
  const cases = [
    ['start-sdk', '0.0.0', ['0.0.1', '0.1.0', '1.0.0']],
    ['start-cli', '9.9.9', ['9.9.10', '9.10.0', '10.0.0']],
    ['start-registry', '1.99.999', ['1.99.1000', '1.100.0', '2.0.0']],
    ['start-tunnel', '99.1.2', ['99.1.3', '99.2.0', '100.0.0']],
    ['start-wrt', '1.2.3', ['1.2.4', '1.3.0', '2.0.0']],
    ['start-os', '0.0.0', ['0.0.0.1', '0.0.1', '0.1.0']],
    ['start-os', '0.4.0', ['0.4.0.1', '0.4.1', '0.5.0']],
    ['start-os', '0.4.0.0', ['0.4.0.1', '0.4.1', '0.5.0']],
    ['start-os', '0.9.9.9', ['0.9.9.10', '0.9.10', '0.10.0']],
    ['start-os', '0.4.99.999', ['0.4.99.1000', '0.4.100', '0.5.0']],
  ]
  for (const [project, baseline, results] of cases)
    TIERS.forEach((tier, index) =>
      assert.equal(nextVersion(project, baseline, [tier]), results[index]),
    )
})
test('invalid tier even after major', () => {
  for (const tier of ['', 'PATCH', 'security', 'minor ', null, 1])
    assert.throws(() => nextVersion('start-sdk', '1.2.3', ['major', tier]))
})
test('malformed versions', () => {
  const invalid = [
    '',
    '1',
    '1.2',
    'v1.2.3',
    '1.2.3.4',
    '01.2.3',
    '1.02.3',
    '1.2.03',
    '-1.2.3',
    '1.2.3\n',
    ' 1.2.3',
    '1.2.3-',
    '1.2.3+',
    '1.2.3-alpha.01',
    '1.2.3-a_1',
    '1.2.3+a_1',
    null,
    123,
  ]
  for (const project of PROJECTS) {
    for (const value of invalid.concat(
      project === 'start-os'
        ? ['0.4', '0.04.0', '0.4.0.03', '0.4.0.1.2', '1.4.0', '1.4.0.3']
        : [],
    ))
      assert.throws(() => nextVersion(project, value, []))
  }
})
test('prereleases are not baselines', () => {
  for (const [project, version] of [
    ['start-sdk', '1.2.3-beta.1'],
    ['start-os', '0.4.0.3-rc.1'],
  ])
    assert.throws(() => nextVersion(project, version, []))
})
test('build metadata is stable for next version', () => {
  assert.equal(nextVersion('start-sdk', '1.2.3+build.1', []), '1.2.3+build.1')
  assert.equal(nextVersion('start-sdk', '1.2.3+build.1', ['patch']), '1.2.4')
})
test('numeric order independent of tag order', () => {
  for (const tags of permutations([
    'start-sdk/v9.99.99',
    'start-sdk/v10.0.0',
    'start-sdk/v9.100.0',
    'start-sdk/v2.0.0',
  ]))
    assert.equal(latestRelease('start-sdk', tags.values()), '10.0.0')
  assert.equal(
    latestRelease('start-sdk', ['start-sdk/v1.9.9', 'start-sdk/v1.10.0']),
    '1.10.0',
  )
  assert.equal(
    latestRelease('start-sdk', ['start-sdk/v1.2.9', 'start-sdk/v1.2.10']),
    '1.2.10',
  )
})
test('exact namespace and stable tags', () => {
  for (const project of PROJECTS) {
    const version = project === 'start-os' ? '0.4.0.3' : '1.2.3'
    const tags = [
      'v99.0.0',
      'other/v99.0.0',
      `${project}-extra/v99.0.0`,
      `refs/tags/${project}/v99.0.0`,
      `${project}/99.0.0`,
      `${project}/V99.0.0`,
      ...['-alpha.1', '-beta.9', '-rc.1', '/extra'].map(
        suffix => `${project}/v${version}${suffix}`,
      ),
      `${project}/v01.2.3`,
      `${project}/v1.2.3.4.5`,
      `${project}/v${version}`,
    ]
    assert.equal(latestRelease(project, tags), version)
  }
})
test('prereleases do not override stable', () => {
  assert.equal(
    latestRelease('start-wrt', ['start-wrt/v1.0.0', 'start-wrt/v2.0.0-beta.4']),
    '1.0.0',
  )
  assert.equal(
    latestRelease('start-os', ['start-os/v0.4.0.3', 'start-os/v0.5.0-rc.1']),
    '0.4.0.3',
  )
})
test('no stable release', () => {
  for (const tags of [
    [],
    ['start-sdk/v1.0.0-beta.1'],
    ['other/v1.0.0'],
    ['start-sdk/vbad'],
    ['start-sdk/v1.2.3.4'],
  ])
    assert.equal(latestRelease('start-sdk', tags), null)
})
test('StartOS normalizes omitted patch for comparison', () => {
  for (const [tags, expected] of [
    [['0.4.0.99', '0.4.1'], '0.4.1'],
    [['0.4.0', '0.4.0.1'], '0.4.0.1'],
    [['0.9.99.99', '0.10.0'], '0.10.0'],
    [['1.2.3.4', '0.4.0.01'], null],
  ])
    assert.equal(
      latestRelease(
        'start-os',
        tags.map(version => `start-os/v${version}`),
      ),
      expected,
    )
})
test('build metadata is stable for latest release', () => {
  assert.equal(
    latestRelease('start-sdk', ['start-sdk/v1.2.3+build.4']),
    '1.2.3+build.4',
  )
})
test('canonical manifest paths', t => {
  const { root, write } = fixture(t)
  write('package.json', '{"version":"0.4.0.3"}')
  write('projects/start-os/Cargo.toml', '[package]\nversion = "0.4.0-rev.3"')
  write('projects/start-os/package.json', '{"version":"9.9.9"}')
  write('projects/start-sdk/package.json', '{"version":"3.0.3"}')
  write('projects/start-sdk/Cargo.toml', '[package]\nversion = "9.9.9"')
  for (const project of ['start-cli', 'start-tunnel', 'start-registry']) {
    write(
      `projects/${project}/Cargo.toml`,
      '[package]\nversion = "1.2.3" # VERSION_BUMP\n',
    )
    assert.equal(manifestVersion(root, project), '1.2.3')
  }
  write('projects/start-wrt/Cargo.toml', '[package]\nversion = "9.9.9"')
  write(
    'projects/start-wrt/backend/ctrl/Cargo.toml',
    '[package]\nversion = "1.3.0"',
  )
  assert.equal(manifestVersion(root, 'start-os'), '0.4.0.3')
  assert.equal(manifestVersion(root, 'start-sdk'), '3.0.3')
  assert.equal(manifestVersion(root, 'start-wrt'), '1.3.0')
})
test('package table only and literal quotes', t => {
  const { root, write } = fixture(t)
  write(
    'projects/start-cli/Cargo.toml',
    `[dependencies]
version = "9.9.9" # VERSION_BUMP
 [ package ] # manifest
name = "start-cli"
  version = '1.2.3-beta.1+build' # VERSION_BUMP
[[bin]]
version = "8.8.8"
[package.metadata]
version = "7.7.7"
`,
  )
  assert.equal(manifestVersion(root, 'start-cli'), '1.2.3-beta.1+build')
})
test('malformed or missing package version', t => {
  const { root, write } = fixture(t)
  for (const text of [
    '[dependencies]\nversion = "1.2.3"',
    '[package]\nname = "cli"',
    '[package]\nversion.workspace = true',
    '[package]\nversion = 123',
    '[package]\nversion = "1.2.3"\nversion = "1.2.4"',
    '[package]\nversion = "1.2.3.4"',
    '[package]\nversion = "1.2.03"',
    '[package]\nversion = "1.2.3-alpha.01"',
  ]) {
    write('projects/start-cli/Cargo.toml', text)
    assert.throws(() => manifestVersion(root, 'start-cli'))
  }
})
test('invalid JSON version', t => {
  const { root, write } = fixture(t)
  for (const text of [
    '{}',
    '[]',
    '{"version":null}',
    '{"version":123}',
    '{"version":"1.2.3.4"}',
    '{',
  ]) {
    write('projects/start-sdk/package.json', text)
    assert.throws(() => manifestVersion(root, 'start-sdk'))
  }
})
test('missing manifest', t => {
  const { root } = fixture(t)
  assert.throws(() => manifestVersion(root, 'start-wrt'), { code: 'ENOENT' })
})
test('unknown project', t => {
  const { root } = fixture(t)
  for (const project of ['other', '../start-os', 'projects/start-sdk', '']) {
    assert.throws(() => manifestVersion(root, project))
    assert.throws(() => nextVersion(project, '1.2.3', []))
    assert.throws(() => latestRelease(project, []))
  }
})
test('arbitrary sized numeric components retain precision', () => {
  assert.equal(
    nextVersion(
      'start-sdk',
      '9007199254740993.9007199254740993.9007199254740993',
      ['patch'],
    ),
    '9007199254740993.9007199254740993.9007199254740994',
  )
  assert.equal(
    nextVersion('start-os', '0.9007199254740993.1', ['major']),
    '0.9007199254740994.0',
  )
  assert.equal(
    latestRelease('start-sdk', [
      'start-sdk/v9007199254740993.0.0',
      'start-sdk/v9007199254740992.0.0',
    ]),
    '9007199254740993.0.0',
  )
})
test('manifest UTF-8 decoding is strict', t => {
  const { root, write } = fixture(t)
  write(MANIFESTS['start-sdk'], Buffer.from([0xff]))
  assert.throws(() => manifestVersion(root, 'start-sdk'))
})
