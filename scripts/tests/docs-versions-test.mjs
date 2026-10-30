import assert from 'node:assert/strict'
import { copyFileSync, mkdirSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import test from 'node:test'
import { reconcile, releaseLabel } from '../docs-versions.mjs'
import { fixture, REPO } from './fixtures.mjs'

const CATALOG = 'shared-libs/crates/start-core/locales/release-notes.json'

function conf(books) {
  return `# book-name=version\n${Object.entries(books)
    .map(([book, version]) => `${book}=${version}`)
    .join('\n')}\n`
}

const PUBLISHED = {
  'start-os': '0.4.0.x',
  'start-tunnel': '1.0.x',
  packaging: '0.4.0.x',
  'bitcoin-guides': '1.0.x',
  'start-wrt': '1.2.x',
  support: '1.0.x',
}
const MASTER = {
  ...PUBLISHED,
  'start-os': '0.4.1.x',
  packaging: '0.4.1.x',
  'start-tunnel': '1.1.x',
  support: '2.0.x',
}

test('a release label keeps the version up to its patch', () => {
  assert.equal(releaseLabel('start-wrt', '1.3.0'), '1.3.x')
  assert.equal(releaseLabel('start-wrt', '1.4.0-beta.1'), '1.4.x')
  assert.equal(releaseLabel('start-os', '0.4.0.4'), '0.4.0.x')
  assert.equal(releaseLabel('start-os', '0.4.1'), '0.4.1.x')
  assert.throws(() => releaseLabel('start-wrt', 'nonsense'))
})

test('a tag labels its own book and holds other products', () => {
  assert.equal(
    reconcile(conf(MASTER), conf(PUBLISHED), 'start-wrt', '1.3.0'),
    conf({ ...PUBLISHED, 'start-wrt': '1.3.x', support: '2.0.x' }),
  )
})

test('a book without a release label takes the tagged line', () => {
  assert.equal(
    reconcile(conf(MASTER), conf(PUBLISHED), 'start-sdk', '3.1.1'),
    conf({ ...PUBLISHED, packaging: '0.4.1.x', support: '2.0.x' }),
  )
})

test('a retained site tree still takes the released book label from its tag', () => {
  assert.equal(
    reconcile(
      conf(MASTER),
      conf(PUBLISHED),
      'start-tunnel',
      '1.5.0',
      conf(PUBLISHED),
    ),
    conf({ ...PUBLISHED, 'start-tunnel': '1.1.x' }),
  )
})

test('a retained site tree keeps a book line its tag does not state', () => {
  const { 'start-tunnel': _, ...withoutTunnel } = MASTER
  assert.equal(
    reconcile(
      conf(withoutTunnel),
      conf(PUBLISHED),
      'start-tunnel',
      '1.5.0',
      conf(PUBLISHED),
    ),
    conf(PUBLISHED),
  )
})

test('an unpublished product stays off the site', () => {
  const { 'start-tunnel': _, ...unpublished } = PUBLISHED
  assert.equal(
    reconcile(conf(MASTER), conf(unpublished), 'start-wrt', '1.3.0'),
    conf({
      ...unpublished,
      'start-wrt': '1.3.x',
      support: '2.0.x',
    }),
  )
})

test('a book the tagged tree dropped keeps its published line', () => {
  const { 'start-os': _, ...tagged } = MASTER
  const result = reconcile(conf(tagged), conf(PUBLISHED), 'start-wrt', '1.3.0')
  assert.match(result, /^start-os=0\.4\.0\.x$/m)
  const { 'start-wrt': __, ...withoutOwn } = MASTER
  assert.match(
    reconcile(conf(withoutOwn), conf(PUBLISHED), 'start-wrt', '1.3.0'),
    /^start-wrt=1\.3\.x$/m,
  )
})

function syncFixture(t) {
  const f = fixture(t)
  const remote = join(f.root, 'remote.git')
  f.git('init', '-q', '--bare', remote)
  f.git('init', '-q')
  f.git('config', 'user.name', 'Fixture')
  f.git('config', 'user.email', 'fixture@example.com')
  f.git('config', 'commit.gpgsign', 'false')
  f.git('checkout', '-qb', 'master')
  f.git('remote', 'add', 'origin', remote)

  const runnerTemp = join(f.root, 'runner-temp')
  for (const path of [
    'scripts/changelog.mjs',
    'scripts/changelog-version.mjs',
    'scripts/docs-versions.mjs',
    CATALOG,
  ]) {
    mkdirSync(dirname(join(runnerTemp, path)), { recursive: true })
    copyFileSync(join(REPO, path), join(runnerTemp, path))
  }
  f.git('config', 'core.excludesFile', join(f.root, 'exclude'))
  f.write('exclude', 'remote.git/\nrunner-temp/\nexclude\n')

  const commit = (message, files) => {
    for (const [path, data] of Object.entries(files)) f.write(path, data)
    f.git('add', '-A')
    f.git('commit', '-qm', message)
  }
  commit('base', {
    'root.md': 'root\n',
    'projects/start-docs/versions.conf': conf(PUBLISHED),
    'projects/start-wrt/docs/page.md': 'base\n',
    'projects/start-sdk/docs/page.md': 'base\n',
    'projects/start-cli/README.md': 'base\n',
    'projects/start-cli/CHANGELOG.md': '# Changelog\n',
  })
  f.git('push', '-q', 'origin', 'HEAD:master', 'HEAD:live-docs')

  const shell = readFileSync(
    join(REPO, '.github/workflows/docs-sync-on-tag.yml'),
    'utf8',
  )
    .split('      - name: Copy the tagged tree onto live-docs\n')[1]
    .split('        run: |\n')[1]
    .split('\n')
    .map(line => line.slice(10))
    .join('\n')
  f.env.RUNNER_TEMP = runnerTemp

  const publish = tag => {
    f.git('-c', 'tag.gpgsign=false', 'tag', tag)
    f.git('push', '-q', 'origin', 'HEAD:master', tag)
  }
  const sync = tag => {
    f.git('fetch', '-q', 'origin')
    const branch = f.git('rev-parse', 'HEAD').trim()
    f.git('checkout', '-q', '--detach', 'origin/live-docs')
    const result = f.run('bash', ['-c', shell], {
      encoding: 'utf8',
      env: { ...f.env, TAG: tag },
    })
    assert.equal(result.status, 0, result.stdout + result.stderr)
    f.git('fetch', '-q', 'origin')
    f.git('checkout', '-qf', branch)
    return f.git('show', 'origin/live-docs:projects/start-docs/versions.conf')
  }
  return { commit, publish, sync }
}

test('an older SDK tag updates its label without rolling back the site tree', t => {
  const { commit, publish, sync } = syncFixture(t)
  commit('sdk 3.1.1', {
    'projects/start-sdk/docs/page.md': '3.1.1\n',
    'projects/start-docs/versions.conf': conf(MASTER),
  })
  publish('start-sdk/v3.1.1')
  commit('wrt 1.3.0', {
    'projects/start-wrt/docs/page.md': '1.3.0\n',
    'projects/start-docs/versions.conf': conf({
      ...MASTER,
      packaging: '0.4.2.x',
      support: '3.0.x',
    }),
  })
  publish('start-wrt/v1.3.0')

  assert.equal(
    sync('start-wrt/v1.3.0'),
    conf({ ...PUBLISHED, 'start-wrt': '1.3.x', support: '3.0.x' }),
  )
  assert.equal(
    sync('start-sdk/v3.1.1'),
    conf({
      ...PUBLISHED,
      packaging: '0.4.1.x',
      'start-wrt': '1.3.x',
      support: '3.0.x',
    }),
  )
})

test('the sync step writes each release onto the published labels', t => {
  const { commit, publish, sync } = syncFixture(t)
  commit('wrt 1.2.1', { 'projects/start-wrt/docs/page.md': '1.2.1\n' })
  publish('start-wrt/v1.2.1')
  commit('wrt 1.3.0', {
    'projects/start-wrt/docs/page.md': '1.3.0\n',
    'projects/start-docs/versions.conf': conf(MASTER),
  })
  publish('start-wrt/v1.3.0')
  commit('sdk 3.1.1', { 'projects/start-sdk/docs/page.md': '3.1.1\n' })
  publish('start-sdk/v3.1.1')
  commit('cli 2.5.2', { 'projects/start-cli/README.md': '2.5.2\n' })
  publish('start-cli/v2.5.2')

  assert.equal(
    sync('start-sdk/v3.1.1'),
    conf({ ...PUBLISHED, packaging: '0.4.1.x', support: '2.0.x' }),
    'a newer tag publishes its own book and the site tree',
  )
  assert.equal(
    sync('start-wrt/v1.3.0'),
    conf({
      ...PUBLISHED,
      packaging: '0.4.1.x',
      support: '2.0.x',
      'start-wrt': '1.3.x',
    }),
    'a tag behind the shared trees still labels its own book',
  )
  assert.equal(
    sync('start-wrt/v1.2.1'),
    conf({
      ...PUBLISHED,
      packaging: '0.4.1.x',
      support: '2.0.x',
      'start-wrt': '1.3.x',
    }),
    'a tag behind its own release leaves the label alone',
  )
  assert.equal(
    sync('start-cli/v2.5.2'),
    conf({
      ...PUBLISHED,
      packaging: '0.4.1.x',
      support: '2.0.x',
      'start-wrt': '1.3.x',
    }),
    'a bookless tag leaves the labels alone',
  )
})
