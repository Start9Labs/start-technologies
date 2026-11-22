import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'
import '@angular/compiler'
import ts from 'typescript'

const configPath = fileURLToPath(
  new URL('../../../../tsconfig.json', import.meta.url),
)
const compilerOptions = ts.convertCompilerOptionsFromJson(
  ts.readConfigFile(configPath, ts.sys.readFile).config.compilerOptions,
  dirname(configPath),
).options

registerHooks({
  resolve(specifier, context, next) {
    if (specifier === '@start9labs/shared') {
      return {
        url: new URL(
          '../../shared/src/services/exver.service.ts',
          import.meta.url,
        ).href,
        shortCircuit: true,
      }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (!url.endsWith('.ts')) return next(url, context)
    return {
      format: 'module',
      source: ts.transpileModule(readFileSync(new URL(url), 'utf8'), {
        compilerOptions,
      }).outputText,
      shortCircuit: true,
    }
  },
})
const { Exver } = await import('../../shared/src/services/exver.service.ts')
const { compatibleVersions } =
  await import('../src/util/compatible-versions.ts')
const { canMigrateFrom, pendingReleases, requiresOsUpdate } =
  await import('../src/util/releases.ts')

const exver = new Exver()
const otherVersions = {
  '#quantum:2.0.2-stable:3': {},
  '#quantum:2.0.1:0': {},
  '2.0.2-stable:3': {},
  '2.0.0:0': {},
}
const flavors = [
  { version: '2.1.0:0', flavor: null, satisfies: [], otherVersions },
  {
    version: '#quantum:2.1.0:0',
    flavor: 'quantum',
    satisfies: ['2.63.23:0'],
    otherVersions,
  },
]

test('a flavor whose best release fails still offers an older release that satisfies', () => {
  const range = '=#quantum:2.0.2-stable:3'
  assert.deepEqual(compatibleVersions(exver, flavors[1], range), [
    '#quantum:2.0.2-stable:3',
  ])
  assert.equal(
    flavors.find(p => compatibleVersions(exver, p, range).length),
    flavors[1],
  )
})

test('the best release matches by its own version or a satisfies alias', () => {
  assert.deepEqual(
    compatibleVersions(exver, flavors[1], '>=#quantum:2.0.1:0'),
    ['#quantum:2.1.0:0', '#quantum:2.0.2-stable:3', '#quantum:2.0.1:0'],
  )
  assert.deepEqual(compatibleVersions(exver, flavors[1], '^2.62.2:1'), [
    '#quantum:2.1.0:0',
  ])
  assert.deepEqual(compatibleVersions(exver, flavors[0], null), [
    '2.1.0:0',
    '2.0.2-stable:3',
    '2.0.0:0',
  ])
})

test('pending notes include superseded revisions, newest first within the installed flavor and version interval', () => {
  const pkg = {
    version: '2.0.0:0',
    flavor: null,
    otherVersions: {
      '0.9.0:0': { releaseNotes: 'Before installed' },
      '1.0.0:0': { releaseNotes: 'Installed' },
      '1.0.0:1': { releaseNotes: 'Revision', superseded: true },
      '1.5.0:0': { releaseNotes: 'Intermediate' },
      '2.1.0:0': { releaseNotes: 'After target' },
      '#quantum:1.5.0:0': { releaseNotes: 'Other flavor' },
    },
  }
  const releases = pendingReleases(exver, pkg, '1.0.0:0')
  assert.deepEqual(
    releases.map(release => release.version),
    ['2.0.0:0', '1.5.0:0', '1.0.0:1'],
  )
  assert.deepEqual(pendingReleases(exver, pkg), [pkg])
})

test('switching marketplace flavors does not present that flavor history as skipped updates', () => {
  assert.deepEqual(pendingReleases(exver, flavors[1], '1.0.0:0'), [flavors[1]])
})

test('no flavor offers a version when no release satisfies the range', () => {
  for (const range of ['=#quantum:3.0.0:0', '>=3.0.0:0']) {
    assert.equal(
      flavors.find(p => compatibleVersions(exver, p, range).length),
      undefined,
    )
  }
})

test('a release accepts an installed version within its source range, and any version without one', () => {
  assert.equal(
    canMigrateFrom(exver, { sourceVersion: '>=1.5.0' }, '1.6.0:0'),
    true,
  )
  assert.equal(
    canMigrateFrom(exver, { sourceVersion: '>=1.5.0' }, '1.0.0:0'),
    false,
  )
  assert.equal(canMigrateFrom(exver, { sourceVersion: null }, '0.1.0:0'), true)
})

test('a release built for a StartOS beyond the compatibility range requires an OS update', () => {
  const compat = '>=0.3.0:0 <=0.4.3:0'
  assert.equal(requiresOsUpdate(exver, { osVersion: '0.4.3' }, compat), false)
  assert.equal(requiresOsUpdate(exver, { osVersion: '0.4.4' }, compat), true)
})
