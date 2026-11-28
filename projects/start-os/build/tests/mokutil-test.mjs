import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import {
  mkdtempSync,
  readFileSync,
  writeFileSync,
  rmSync,
  mkdirSync,
  symlinkSync,
} from 'node:fs'
import { createHash } from 'node:crypto'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'

process.umask(0o077)
const vendor = '605dab50-e046-4300-abb6-3dd810dd8b23'
const variable = (directory, name) => join(directory, `${name}-${vendor}`)
const oldHash = execFileSync('openssl', [
  'passwd',
  '-6',
  '-salt',
  'old',
  'fixture-old',
])
const newHash = execFileSync('openssl', [
  'passwd',
  '-6',
  '-salt',
  'new',
  'fixture-new',
])

function createCertificate(root, basename) {
  const certificate = join(root, `${basename}.der`)
  execFileSync(
    'openssl',
    [
      'req',
      '-new',
      '-x509',
      '-newkey',
      'rsa:2048',
      '-nodes',
      '-keyout',
      join(root, `${basename}.pem`),
      '-outform',
      'DER',
      '-out',
      certificate,
      '-subj',
      `/CN=${basename}`,
    ],
    { stdio: 'ignore' },
  )
  return certificate
}

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'mokutil-'))
  const efivars = join(root, 'efivars')
  mkdirSync(efivars)
  writeFileSync(
    join(efivars, 'SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c'),
    Buffer.from([7, 0, 0, 0, 0]),
  )
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const hashFile = join(root, 'password-hash')
  writeFileSync(hashFile, oldHash)
  const certificate = createCertificate(root, 'certificate')
  writeFileSync(join(root, 'new.hash'), newHash)
  const run = (args, binary = 'mokutil') =>
    spawnSync(binary, args, {
      env: { ...process.env, EFIVARFS_PATH: `${efivars}/`, LC_ALL: 'C' },
      encoding: 'utf8',
      timeout: 15000,
    })
  const imported = run([
    '--import',
    certificate,
    '--hash-file',
    hashFile,
    '--ignore-keyring',
  ])
  assert.equal(imported.status, 0, imported.stderr)
  return { root, efivars, hashFile, certificate, run }
}

test('auth-only refresh preserves every pending byte and uses the native hash codec', t => {
  const f = fixture(t)
  const auth = variable(f.efivars, 'MokAuth')
  const request = variable(f.efivars, 'MokNew')
  const previousAuth = readFileSync(auth)
  // The authentication operation treats request payloads as opaque.
  const opaque = Buffer.concat([
    readFileSync(request),
    Buffer.from('opaque request tail'),
  ])
  writeFileSync(request, opaque)
  writeFileSync(f.hashFile, newHash)
  const result = f.run(['--refresh-import-auth', '--hash-file', f.hashFile])
  assert.equal(result.status, 0, result.stderr)
  assert.deepEqual(readFileSync(request), opaque)
  assert.notDeepEqual(readFileSync(auth), previousAuth)

  const expected = fixture(t)
  writeFileSync(expected.hashFile, newHash)
  rmSync(variable(expected.efivars, 'MokNew'))
  rmSync(variable(expected.efivars, 'MokAuth'))
  const imported = expected.run([
    '--import',
    expected.certificate,
    '--hash-file',
    expected.hashFile,
    '--ignore-keyring',
  ])
  assert.equal(imported.status, 0, imported.stderr)
  assert.deepEqual(
    readFileSync(auth),
    readFileSync(variable(expected.efivars, 'MokAuth')),
  )
  const retry = f.run(['--refresh-import-auth', '--hash-file', f.hashFile])
  assert.equal(retry.status, 0, retry.stderr)
  assert.deepEqual(readFileSync(request), opaque)
})

test('invalid authentication and absent/empty requests do not mutate firmware inputs', t => {
  const f = fixture(t)
  const auth = variable(f.efivars, 'MokAuth')
  const request = variable(f.efivars, 'MokNew')
  const previousAuth = readFileSync(auth)
  const previousRequest = readFileSync(request)
  writeFileSync(f.hashFile, 'not a password hash')
  assert.notEqual(
    f.run(['--refresh-import-auth', '--hash-file', f.hashFile]).status,
    0,
  )
  assert.deepEqual(readFileSync(auth), previousAuth)
  assert.deepEqual(readFileSync(request), previousRequest)
  writeFileSync(f.hashFile, newHash)
  rmSync(request)
  assert.notEqual(
    f.run(['--refresh-import-auth', '--hash-file', f.hashFile]).status,
    0,
  )
  assert.deepEqual(readFileSync(auth), previousAuth)
  writeFileSync(request, Buffer.from([7, 0, 0, 0]))
  assert.notEqual(
    f.run(['--refresh-import-auth', '--hash-file', f.hashFile]).status,
    0,
  )
  assert.deepEqual(readFileSync(auth), previousAuth)
})

test('failed native authentication writes preserve shared imports; stock cleanup loses them', t => {
  for (const binary of ['mokutil', '/unpatched-mokutil']) {
    const f = fixture(t)
    const request = variable(f.efivars, 'MokNew')
    const before = readFileSync(request)
    const auth = variable(f.efivars, 'MokAuth')
    rmSync(auth)
    mkdirSync(auth)
    const certificate = createCertificate(f.root, 'other')
    const result = f.run(
      ['--import', certificate, '--hash-file', f.hashFile, '--ignore-keyring'],
      binary,
    )
    assert.notEqual(result.status, 0)
    assert.match(result.stderr, /Failed to write MokAuth/)
    if (binary === 'mokutil') {
      const after = readFileSync(request)
      assert(after.length > before.length)
      assert.deepEqual(after.subarray(-before.length + 4), before.subarray(4))
      assert.notEqual(
        f.run(['--refresh-import-auth', '--hash-file', f.hashFile]).status,
        0,
      )
      assert.deepEqual(readFileSync(request), after)
    } else {
      assert.throws(() => readFileSync(request), { code: 'ENOENT' })
    }
  }
})

const productionBinary = process.env.FDE_MOK_TEST_BINARY
for (const name of [
  'native_pending_refresh_preserves_subjects_and_changes_authentication',
  'native_unpatched_tool_rejects_before_request_mutation',
  'native_auth_failure_preserves_other_subjects_and_retry_finishes',
]) {
  test(`production ${name}`, { skip: !productionBinary }, t => {
    const f = fixture(t)
    const other = createCertificate(f.root, 'other')
    const env = {
      ...process.env,
      FDE_MOK_NATIVE_ROOT: f.root,
      EFIVARFS_PATH: `${f.efivars}/`,
      LC_ALL: 'C',
    }
    if (
      name ===
      'native_pending_refresh_preserves_subjects_and_changes_authentication'
    ) {
      const imported = f.run([
        '--import',
        other,
        '--hash-file',
        f.hashFile,
        '--ignore-keyring',
      ])
      assert.equal(imported.status, 0, imported.stderr)
    }
    if (name === 'native_unpatched_tool_rejects_before_request_mutation') {
      const bin = join(f.root, 'bin')
      mkdirSync(bin)
      symlinkSync('/unpatched-mokutil', join(bin, 'mokutil'))
      env.PATH = `${bin}:${process.env.PATH}`
    }
    const result = spawnSync(
      productionBinary,
      [
        '--exact',
        `util::mok::enrollment_tests::${name}`,
        '--ignored',
        '--nocapture',
      ],
      { env, encoding: 'utf8', timeout: 30000 },
    )
    assert.equal(result.status, 0, result.stdout + result.stderr)
    assert.match(result.stdout, /1 passed; 0 failed/)
  })
}

test('source archive contains the actual patched native owner and package identity', () => {
  assert.deepEqual(
    execFileSync('dpkg-query', ['-W', '-f=${binary:Package}\\n']),
    readFileSync('/packages.before'),
  )
  execFileSync('diff', ['-r', '/apt-sources.before', '/etc/apt/sources.list.d'])
  assert.match(
    execFileSync('dpkg-query', ['-W', '-f=${Version}', 'mokutil'], {
      encoding: 'utf8',
    }),
    /\+start9\.1$/,
  )
  const archive = '/usr/share/doc/mokutil/startos-source.tar.gz'
  const source = execFileSync('tar', ['-xOf', archive, './src/mokutil.c'], {
    encoding: 'utf8',
  })
  assert.match(source, /refresh_import_auth/)
  const patch = execFileSync(
    'tar',
    ['-xOf', archive, './debian/patches/startos-shared-auth.patch'],
    { encoding: 'utf8' },
  )
  assert.match(patch, /GPL-3\.0-or-later/)
  assert.match(
    execFileSync('tar', ['-xOf', archive, './debian/rules'], {
      encoding: 'utf8',
    }),
    /dh/,
  )
  console.log(
    'Native mokutil SHA256:',
    createHash('sha256').update(readFileSync('/usr/bin/mokutil')).digest('hex'),
  )
})
