import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../../../../', import.meta.url))
const read = file => readFileSync(path.join(root, file), 'utf8')
const upgrade = read('projects/start-os/build/lib/scripts/upgrade')
const preserve = upgrade.match(
  /^preserve_server_identity\(\) \{[\s\S]*?^\}/m,
)?.[0]
assert.ok(preserve)
const constant = (file, name) => {
  const match = read(file).match(new RegExp(`const ${name}: &str = "([^"]+)"`))
  assert.ok(match, name)
  return match[1]
}
const runtimePath = constant(
  'shared-libs/crates/start-core/src/developer/mod.rs',
  'OS_ID_KEY_PATH',
)
const persistentPath = constant(
  'shared-libs/crates/start-core/src/context/startup.rs',
  'IDENTITY_PATH',
)

function fixture(t) {
  const directory = mkdtempSync(
    path.join(tmpdir(), 'startos-upgrade-identity-'),
  )
  t.after(() => rmSync(directory, { recursive: true, force: true }))
  const config = path.join(directory, 'config')
  mkdirSync(config)
  const runtime = path.join(directory, 'runtime.pem')
  const persistent = path.join(config, path.basename(persistentPath))
  return {
    directory,
    config,
    runtime,
    persistent,
    marker: path.join(config, 'disk.guid'),
    run(env = {}) {
      const result = spawnSync(
        'bash',
        [
          '-ec',
          `${preserve}\npreserve_server_identity "$1" "$2"`,
          'fixture',
          runtime,
          persistent,
        ],
        { encoding: 'utf8', env: { ...process.env, ...env } },
      )
      assert.equal(result.signal, null, result.stderr)
      assert.ok(readdirSync(config).every(name => !name.includes('.tmp.')))
      return result
    },
    fail(command, body = 'exit 73') {
      const bin = path.join(directory, 'bin')
      mkdirSync(bin)
      writeFileSync(path.join(bin, command), `#!/bin/bash\n${body}\n`, {
        mode: 0o755,
      })
      return { PATH: `${bin}:${process.env.PATH}` }
    },
  }
}

function key(file) {
  execFileSync('openssl', ['genpkey', '-algorithm', 'ED25519', '-out', file], {
    stdio: 'ignore',
  })
  return readFileSync(file, 'utf8')
}

test('the OTA script seeds the native identity paths before modifying boot files or activating the image', () => {
  const call = `preserve_server_identity ${runtimePath} ${persistentPath}`
  const seed = upgrade.indexOf(call)
  assert.ok(seed >= 0)
  assert.ok(seed < upgrade.indexOf('unsquashfs -n -f -d /'))
  assert.ok(seed < upgrade.indexOf('ln -rsf '))
})

test('the running account key replaces a stale copy atomically with private permissions', t => {
  const f = fixture(t)
  const expected = key(f.runtime)
  key(f.persistent)
  chmodSync(f.persistent, 0o644)
  writeFileSync(f.marker, 'installed')
  const result = f.run()
  assert.equal(result.status, 0, result.stderr)
  assert.equal(readFileSync(f.persistent, 'utf8'), expected)
  assert.equal(statSync(f.persistent).mode & 0o777, 0o600)
})

test('diagnostic updates preserve an existing config key without a runtime export', t => {
  const f = fixture(t)
  const expected = key(f.persistent)
  chmodSync(f.persistent, 0o640)
  writeFileSync(f.marker, 'installed')
  const result = f.run()
  assert.equal(result.status, 0, result.stderr)
  assert.equal(readFileSync(f.persistent, 'utf8'), expected)
  assert.equal(statSync(f.persistent).mode & 0o777, 0o600)
})

test('a configured server with neither key refuses activation rather than inventing an identity', t => {
  const f = fixture(t)
  writeFileSync(f.marker, 'installed')
  const result = f.run()
  assert.notEqual(result.status, 0)
  assert.match(result.stderr, /Cannot preserve server identity/)
  assert.equal(existsSync(f.persistent), false)
})

test('an unconfigured installer needs no historical key to migrate', t => {
  const f = fixture(t)
  const result = f.run()
  assert.equal(result.status, 0, result.stderr)
  assert.equal(existsSync(f.persistent), false)
})

test('a malformed runtime export cannot replace the saved key', t => {
  const f = fixture(t)
  const expected = key(f.persistent)
  writeFileSync(f.runtime, 'not a private key')
  const result = f.run()
  assert.notEqual(result.status, 0)
  assert.equal(readFileSync(f.persistent, 'utf8'), expected)
})

test('a malformed saved key cannot be used as a diagnostic update source', t => {
  const f = fixture(t)
  writeFileSync(f.marker, 'installed')
  writeFileSync(f.persistent, 'not a private key')
  const result = f.run()
  assert.notEqual(result.status, 0)
  assert.equal(readFileSync(f.persistent, 'utf8'), 'not a private key')
})

test('directory sync failure stops the update after publishing the account key', t => {
  const f = fixture(t)
  const expected = key(f.runtime)
  key(f.persistent)
  const realSync = execFileSync('bash', ['-c', 'command -v sync'], {
    encoding: 'utf8',
  }).trim()
  const result = f.run({
    ...f.fail('sync', '[ ! -d "$1" ] || exit 73\nexec "$REAL_SYNC" "$@"'),
    REAL_SYNC: realSync,
  })
  assert.equal(result.status, 73, result.stderr)
  assert.equal(readFileSync(f.persistent, 'utf8'), expected)
  assert.equal(statSync(f.persistent).mode & 0o777, 0o600)
})

for (const command of ['install', 'sync', 'mv'])
  test(`${command} failure preserves the saved key and cleans the staged copy`, t => {
    const f = fixture(t)
    key(f.runtime)
    const expected = key(f.persistent)
    const result = f.run(f.fail(command))
    assert.equal(result.status, 73, result.stderr)
    assert.equal(readFileSync(f.persistent, 'utf8'), expected)
  })
