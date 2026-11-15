import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { after, before, test } from 'node:test'
import { fixture } from '../../../../scripts/tests/fixtures.mjs'
import {
  archiveHash,
  CHANGELOG,
  chooseCandidate,
  PINS,
  POOL,
  prepare,
  readPins,
  RELEASES,
} from './update.mjs'

const sha256 = value => createHash('sha256').update(value).digest('hex')
let home, signer, key
const gpg = args =>
  execFileSync('gpg', ['--batch', '--homedir', home, ...args], {
    stdio: 'pipe',
  })

before(() => {
  home = mkdtempSync(join(tmpdir(), 'nut-test-keys-'))
  gpg([
    '--pinentry-mode',
    'loopback',
    '--passphrase',
    '',
    '--quick-generate-key',
    'NUT updater fixture <nut@example.invalid>',
    'ed25519',
    'sign',
    '0',
  ])
  signer = gpg(['--with-colons', '--list-keys'])
    .toString()
    .split('\n')
    .find(line => line.startsWith('fpr:'))
    .split(':')[9]
  key = gpg(['--armor', '--export', signer])
})

after(() => {
  if (home) {
    execFileSync('gpgconf', ['--homedir', home, '--kill', 'gpg-agent'])
    rmSync(home, { recursive: true, force: true })
  }
})

function setup(t) {
  const f = fixture(t)
  const pins = {
    NUT_VERSION: '2.8.5',
    NUT_BUILD_IMAGE: 'debian:trixie-slim@sha256:' + '1'.repeat(64),
    NUT_DEBIAN_VERSION: '2.8.5-1',
    NUT_PACKAGE_VERSION: '2.8.5-1~startos13.7',
    NUT_SIGNER: signer,
    NUT_DSC_SHA256: '0'.repeat(64),
    NUT_SOURCE_SHA256: '0'.repeat(64),
    NUT_DEBIAN_SHA256: '0'.repeat(64),
    NUT_PACKAGES:
      "'nut-client nut-server libupsclient7 libnutclient2t64 libnutscan4'",
  }
  f.write(
    PINS,
    Object.entries(pins)
      .map(([name, value]) => `${name}=${value}\n`)
      .join(''),
  )
  mkdirSync(join(f.root, CHANGELOG), { recursive: true })
  const orig = Buffer.from('upstream archive fixture'),
    debian = Buffer.from('Debian packaging fixture')
  const originalHash = sha256(orig),
    debianHash = sha256(debian)
  const s = {
    ...f,
    candidate: '2.8.6-1',
    orig,
    debian,
    originalHash,
    release: {
      tag_name: 'v2.8.6',
      draft: false,
      prerelease: false,
      assets: [{ name: 'nut-2.8.6.tar.gz', digest: 'sha256:' + originalHash }],
    },
    index: '<a href="nut_2.8.6-1.dsc">new</a>',
    tagged: {},
    calls: [],
  }
  s.manifest = `Format: 3.0 (quilt)\nSource: nut\nVersion: ${s.candidate}\nBinary: nut-client, nut-server, libupsclient7, libnutclient2t64, libnutscan4\nChecksums-Sha256:\n ${originalHash} ${orig.length} nut_2.8.6.orig.tar.gz\n ${debianHash} ${debian.length} nut_${s.candidate}.debian.tar.xz\n`
  s.sign = () =>
    execFileSync(
      'gpg',
      [
        '--batch',
        '--homedir',
        home,
        '--pinentry-mode',
        'loopback',
        '--passphrase',
        '',
        '--armor',
        '--clearsign',
      ],
      { input: s.manifest, stdio: ['pipe', 'pipe', 'pipe'] },
    )
  s.get = async url => {
    s.calls.push(url)
    if (s.tagged[url]) return Buffer.from(JSON.stringify(s.tagged[url]))
    if (url.startsWith(RELEASES)) return Buffer.from(JSON.stringify(s.release))
    if (url === POOL) return Buffer.from(s.index)
    if (url.includes('keyring.debian.org')) return key
    if (url.endsWith('.dsc')) return s.signed ?? s.sign()
    assert.fail('Unexpected download: ' + url)
  }
  s.archive = async (url, size) => {
    const data = url.endsWith('.orig.tar.gz') ? s.orig : s.debian
    assert.equal(size, data.length)
    return sha256(data)
  }
  s.prepare = () => prepare(f.root, s.get, s.archive)
  return s
}

test('independent automation waits without network access when packaging is absent', async t => {
  const f = fixture(t)
  const result = await prepare(f.root, () =>
    assert.fail('Unexpected network access'),
  )
  assert.equal(result.report.available, false)
  assert.equal(result.report.changed, false)
  assert.match(result.report.reason, /waiting for package integration/)
  assert.deepEqual(result.changes, {})
})

test('verified upgrade preserves trust and builder pins without writing files', async t => {
  const s = setup(t),
    original = readFileSync(join(s.root, PINS), 'utf8')
  const { report, changes } = await s.prepare()
  assert.equal(report.changed, true)
  assert.equal(report.packageVersion, '2.8.6-1~startos13.1')
  const pins = readPins(changes[PINS]),
    before = readPins(original)
  for (const field of ['NUT_SIGNER', 'NUT_BUILD_IMAGE', 'NUT_PACKAGES'])
    assert.equal(pins[field], before[field])
  assert.equal(pins.NUT_SOURCE_SHA256, s.originalHash)
  assert.ok(Object.hasOwn(changes, `${CHANGELOG}/patch-changed-nut-update.md`))
  assert.equal(readFileSync(join(s.root, PINS), 'utf8'), original)
})

test('updates the pending feature instead of adding another fragment', async t => {
  const s = setup(t),
    name = `${CHANGELOG}/minor-added-ups-monitor.md`
  s.write(
    name,
    '---\nrelease-notes: true\n---\n\n- Includes NUT 2.8.5 for broader UPS compatibility.\n',
  )
  const { changes } = await s.prepare()
  assert.deepEqual(Object.keys(changes), [PINS, name])
  assert.match(changes[name], /Includes NUT 2.8.6/)
})

test('applying a verified update is idempotent', async t => {
  const s = setup(t),
    result = await s.prepare()
  for (const [name, value] of Object.entries(result.changes))
    s.write(name, value)
  const next = await s.prepare()
  assert.equal(next.report.changed, false)
  assert.deepEqual(next.changes, {})
})

test('current-line maintenance can land while waiting for a later upstream release', async t => {
  const s = setup(t)
  s.candidate = '2.8.5-2'
  s.index = '<a href="nut_2.8.5-2.dsc">maintenance</a>'
  s.manifest = s.manifest
    .replaceAll('2.8.6-1', s.candidate)
    .replace('nut_2.8.6.orig', 'nut_2.8.5.orig')
  s.tagged[RELEASES + '/tags/v2.8.5'] = {
    ...s.release,
    tag_name: 'v2.8.5',
    assets: [{ name: 'nut-2.8.5.tar.gz', digest: 'sha256:' + s.originalHash }],
  }
  const { report, changes } = await s.prepare()
  assert.equal(report.waitingForDebian, true)
  assert.equal(report.packageVersion, '2.8.5-2~startos13.1')
  assert.match(
    changes[`${CHANGELOG}/patch-changed-nut-update.md`],
    /Debian maintenance updates/,
  )
})

test('the current pin is a no-op', async t => {
  const s = setup(t)
  s.index = '<a href="nut_2.8.5-1.dsc">current</a>'
  s.release.tag_name = 'v2.8.5'
  const { report, changes } = await s.prepare()
  assert.equal(report.changed, false)
  assert.equal(report.waitingForDebian, false)
  assert.deepEqual(changes, {})
  assert.equal(s.calls.length, 2)
})

test('a missing Debian source reports waiting and preserves pins', async t => {
  const s = setup(t)
  s.index = '<a href="nut_2.8.5-1.dsc">current</a>'
  const { report, changes } = await s.prepare()
  assert.equal(report.waitingForDebian, true)
  assert.equal(report.changed, false)
  assert.deepEqual(changes, {})
})

test('Debian revisions use dpkg ordering', () => {
  const index =
    '<a href="nut_2.8.5-10.dsc">a</a><a href="nut_2.8.6-1~rc1.dsc">b</a><a href="nut_2.8.4-99.dsc">c</a>'
  assert.equal(
    chooseCandidate(index, { NUT_DEBIAN_VERSION: '2.8.5-9' }, '2.8.5'),
    '2.8.5-10',
  )
})

test('pool links cannot escape or select prerelease upstream versions', async t => {
  const s = setup(t)
  s.index =
    '<a href="../nut_2.8.6-1.dsc">a</a><a href="nut_2.9.0~rc1-1.dsc">b</a><a href="https://evil.invalid/nut_2.8.6-1.dsc">c</a>'
  assert.equal((await s.prepare()).report.changed, false)
})

test('rejects prerelease API responses', async t => {
  const s = setup(t)
  s.release.prerelease = true
  await assert.rejects(s.prepare(), /not stable/)
})

test('tampered signed manifests are rejected', async t => {
  const s = setup(t)
  s.signed = Buffer.from(
    s.sign().toString().replace('Source: nut', 'Source: bad'),
  )
  await assert.rejects(s.prepare(), /gpgv/)
})

test('a valid signature from a different pinned signer is rejected', async t => {
  const s = setup(t)
  s.write(
    PINS,
    readFileSync(join(s.root, PINS), 'utf8').replace(signer, 'A'.repeat(40)),
  )
  await assert.rejects(s.prepare(), /pinned maintainer/)
})

test('modified archive bytes fail checksum verification', async t => {
  const s = setup(t)
  s.orig = Buffer.from(s.orig.toString().replace('upstream', 'modified'))
  await assert.rejects(s.prepare(), /checksum mismatch/)
})

test('upstream digests must match Debian source', async t => {
  const s = setup(t)
  s.release.assets[0].digest = 'sha256:' + '0'.repeat(64)
  await assert.rejects(s.prepare(), /release digest/)
})

test('missing runtime packages require manual review', async t => {
  const s = setup(t)
  s.manifest = s.manifest.replace('libnutscan4', 'libnutscan5')
  await assert.rejects(s.prepare(), /package names changed/)
})

test('changed source identity requires manual review', async t => {
  const s = setup(t)
  s.manifest = s.manifest.replace('Source: nut', 'Source: another')
  await assert.rejects(s.prepare(), /identity or format/)
})

test('archive hashing enforces signed sizes and HTTP status', async () => {
  const response = bytes => async () => new Response(bytes)
  await assert.rejects(
    archiveHash('https://example.invalid/archive', 3, response('too long')),
    /exceeds signed size/,
  )
  await assert.rejects(
    archiveHash('https://example.invalid/archive', 20, response('short')),
    /does not match signed/,
  )
  await assert.rejects(
    archiveHash('https://example.invalid/archive', 0, response('')),
    /unsupported size/,
  )
  await assert.rejects(
    archiveHash(
      'https://example.invalid/archive',
      3,
      async () => new Response('', { status: 404 }),
    ),
    /HTTP 404/,
  )
  assert.equal(
    await archiveHash('https://example.invalid/archive', 3, response('abc')),
    sha256('abc'),
  )
})

test('pin parsing rejects shell expansion and duplicate values', () => {
  for (const text of [
    'NUT_VERSION=$(id)\n',
    'NUT_VERSION="$(id)"\n',
    'NUT_VERSION=a\nNUT_VERSION=b\n',
  ])
    assert.throws(() => readPins(text), /Malformed or duplicate/)
})
