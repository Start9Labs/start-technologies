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
    forget(env = {}) {
      return spawnSync(
        'bash',
        [
          path.join(
            root,
            'projects/start-os/build/lib/scripts/forget-server-identity',
          ),
          config,
        ],
        { encoding: 'utf8', env: { ...process.env, ...env } },
      )
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

test('emulated reflash delegates identity cleanup to the staged native owner', () => {
  const result = spawnSync(
    'make',
    [
      '--no-print-directory',
      '--dry-run',
      'start-os-emulate-reflash',
      'STARTOS_TARGETS=',
      'REMOTE=fixture',
      'MAKE=true',
    ],
    { cwd: root, encoding: 'utf8' },
  )
  assert.equal(result.status, 0, result.stderr)
  assert.match(
    result.stdout,
    /sudo \/media\/startos\/next\/usr\/lib\/startos\/scripts\/forget-server-identity/,
  )
  assert.doesNotMatch(result.stdout, /sudo rm -f[^\n]*disk\.guid/)
})

test('deployment recipes take their staged root from the shared workspace owner', () => {
  for (const target of ['start-os-update', 'start-os-emulate-reflash']) {
    const result = spawnSync(
      'make',
      [
        '--no-print-directory',
        '--dry-run',
        target,
        'STARTOS_TARGETS=',
        'REMOTE=fixture',
        'MAKE=true',
        'STARTOS_REPACK_ROOT=/owned/repack-root',
      ],
      { cwd: root, encoding: 'utf8' },
    )
    assert.equal(result.status, 0, result.stderr)
    assert.match(result.stdout, /DESTDIR=\/owned\/repack-root/)
    assert.match(
      result.stdout,
      /sudo \/owned\/repack-root\/usr\/lib\/startos\/scripts\/chroot-and-upgrade/,
    )
    assert.doesNotMatch(result.stdout, /\/media\/startos\/next/)
  }
})

test('upgrade refreshes missing and legacy workspace helpers before using their paths', t => {
  const start = upgrade.indexOf('if [ ! -f "$SOURCE_DIR/os-image-store" ]')
  const stop = upgrade.indexOf(
    'startos_reserve_image_store /media/startos/config',
  )
  assert.ok(start > 0 && stop > start)
  const loader = upgrade.slice(start, stop)
  const helper = path.join(
    root,
    'projects/start-os/build/lib/scripts/os-image-store',
  )
  for (const initial of ['missing', 'legacy', 'current']) {
    const directory = mkdtempSync(
      path.join(tmpdir(), 'startos-workspace-loader-'),
    )
    t.after(() => rmSync(directory, { recursive: true, force: true }))
    const trace = path.join(directory, 'extract.log')
    const volume = path.join(directory, 'volume with space')
    for (const name of [
      'os-public-source-cache-name',
      'os-public-source-prune-name',
    ]) {
      writeFileSync(path.join(directory, name), 'present\n')
    }
    if (initial !== 'missing') {
      writeFileSync(
        path.join(directory, 'os-image-store'),
        initial === 'current'
          ? readFileSync(helper)
          : 'startos_reserve_image_store() { :; }\n',
      )
    }
    const result = spawnSync(
      'bash',
      [
        '-ec',
        `
SOURCE_DIR=$1
unsquashfs() {
    printf '%s\\n' "$*" >> "$TRACE"
    cp "$HELPER" "$SOURCE_DIR/os-image-store"
}
${loader}
startos_repack_paths "$2"
`,
        'workspace-loader',
        directory,
        volume,
      ],
      {
        encoding: 'utf8',
        env: { ...process.env, TRACE: trace, HELPER: helper },
      },
    )
    assert.equal(result.status, 0, result.stderr)
    assert.deepEqual(result.stdout.split('\0'), [
      path.join(volume, 'next'),
      path.join(volume, 'upper'),
      '',
    ])
    assert.equal(existsSync(trace), initial !== 'current')
    if (initial !== 'current') {
      assert.match(
        readFileSync(trace, 'utf8'),
        /-n -f -d \/.*\/usr\/lib\/startos\/scripts\/os-image-store/,
      )
    }
  }
})

test('private repacks retire partial output before admission and stop on admission failure', t => {
  const source = read('projects/start-os/build/lib/scripts/chroot-and-upgrade')
  const prepare = source.match(/^prepare_repack_output\(\) \{[\s\S]*?^\}/m)?.[0]
  assert.ok(prepare)
  assert.match(
    source,
    /prepare_repack_output "\$REPACK_IMAGE" "\$REPACK_ROOT" \/media\/startos\/config \|\| exit/,
  )
  for (const status of [0, 73]) {
    const directory = mkdtempSync(
      path.join(tmpdir(), 'startos-repack-admission-'),
    )
    t.after(() => rmSync(directory, { recursive: true, force: true }))
    const tree = path.join(directory, 'root with space')
    const config = path.join(directory, 'config')
    const output = path.join(directory, 'next.squashfs')
    const trace = path.join(directory, 'admission')
    const built = path.join(directory, 'build-entered')
    mkdirSync(tree)
    mkdirSync(config)
    writeFileSync(path.join(tree, 'private'), 'retained workspace data')
    writeFileSync(path.join(config, 'selected.rootfs'), 'selected')
    execFileSync('ln', [
      '-s',
      'selected.rootfs',
      path.join(config, 'current.rootfs'),
    ])
    writeFileSync(output, 'incomplete SquashFS')
    writeFileSync(path.join(directory, 'unrelated'), 'keep')
    writeFileSync(
      path.join(directory, 'prune-images'),
      `#!/bin/bash
[ ! -e "$OUTPUT" ] || exit 91
printf '%s\\n' "$1" > "$TRACE"
exit "$STATUS"
`,
      { mode: 0o755 },
    )
    const result = spawnSync(
      'bash',
      [
        '-ec',
        `SOURCE_DIR=$1
${prepare}
prepare_repack_output "$OUTPUT" "$TREE" "$CONFIG" || exit
printf built > "$BUILT"
`,
        'repack-admission',
        directory,
      ],
      {
        encoding: 'utf8',
        env: {
          ...process.env,
          OUTPUT: output,
          TREE: tree,
          CONFIG: config,
          TRACE: trace,
          STATUS: String(status),
          BUILT: built,
        },
      },
    )
    assert.equal(result.status, status, result.stderr)
    assert.equal(existsSync(output), false)
    assert.equal(existsSync(built), status === 0)
    assert.match(readFileSync(trace, 'utf8'), /^\d+\n$/)
    assert.equal(
      readFileSync(path.join(directory, 'unrelated'), 'utf8'),
      'keep',
    )
  }
})

test('the OTA script reserves image storage before seeding identity and changing boot state', () => {
  const call = `preserve_server_identity ${runtimePath} ${persistentPath}`
  const seed = upgrade.indexOf(call)
  const reserve = upgrade.indexOf(
    'startos_reserve_image_store /media/startos/config',
  )
  assert.ok(reserve >= 0)
  assert.ok(reserve < seed)
  assert.ok(seed < upgrade.indexOf('unsquashfs -f -d / "$1" boot'))
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

test('an update without a runtime export leaves the saved key untouched', t => {
  const f = fixture(t)
  const expected = key(f.persistent)
  writeFileSync(f.marker, 'installed')
  const result = f.run()
  assert.equal(result.status, 0, result.stderr)
  assert.equal(readFileSync(f.persistent, 'utf8'), expected)
})

test('a configured server without a runtime export updates without inventing an identity', t => {
  const f = fixture(t)
  writeFileSync(f.marker, 'installed')
  const result = f.run()
  assert.equal(result.status, 0, result.stderr)
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

test('a malformed saved key does not block an update without a runtime export', t => {
  const f = fixture(t)
  writeFileSync(f.marker, 'installed')
  writeFileSync(f.persistent, 'not a private key')
  const result = f.run()
  assert.equal(result.status, 0, result.stderr)
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

test('forgetting a server removes account and setup keys before clearing its marker', t => {
  const f = fixture(t)
  key(f.persistent)
  const provisional = path.join(f.config, 'setup.key.pem')
  key(provisional)
  writeFileSync(f.marker, 'installed')
  const result = f.forget()
  assert.equal(result.status, 0, result.stderr)
  assert.equal(existsSync(f.persistent), false)
  assert.equal(existsSync(provisional), false)
  assert.equal(existsSync(f.marker), false)
})

test('forgetting cannot clear the marker before key removal is durable', t => {
  const f = fixture(t)
  key(f.persistent)
  const provisional = path.join(f.config, 'setup.key.pem')
  key(provisional)
  writeFileSync(f.marker, 'installed')
  const result = f.forget(f.fail('sync'))
  assert.equal(result.status, 73, result.stderr)
  assert.equal(existsSync(f.persistent), false)
  assert.equal(existsSync(provisional), false)
  assert.equal(readFileSync(f.marker, 'utf8'), 'installed')
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
