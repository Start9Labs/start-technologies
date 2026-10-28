import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'

const source = join(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  'check-consumers.sh',
)

function check(previous, candidate) {
  const root = mkdtempSync(join(tmpdir(), 'sdk-consumers-test-'))
  try {
    const sdk = join(root, 'sdk')
    const pkg = join(root, 'consumer')
    const bin = join(root, 'bin')
    const fixture = join(root, 'fixture')
    for (const directory of [
      join(sdk, 'dist'),
      join(pkg, 'node_modules', '.bin'),
      bin,
      join(fixture, 'package'),
    ]) {
      mkdirSync(directory, { recursive: true })
    }
    copyFileSync(source, join(sdk, 'check-consumers.sh'))
    writeFileSync(join(sdk, 'dist', 'package.json'), '{}')
    writeFileSync(join(fixture, 'package', 'package.json'), '{}')
    writeFileSync(join(pkg, 'tsconfig.json'), '{}')
    writeFileSync(
      join(bin, 'npm'),
      '#!/usr/bin/env bash\ntar czf sdk.tgz -C "$SDK_FIXTURE" package\n',
    )
    writeFileSync(
      join(pkg, 'node_modules', '.bin', 'tsc'),
      `#!/usr/bin/env bash
case "$2" in
  */previous/*) mode=$PREVIOUS ;;
  */candidate/*) mode=$CANDIDATE ;;
esac
case "$mode" in
  clean) exit 0 ;;
  diagnostic) echo 'startos/main.ts(1,1): error TS2322: Type string is not assignable to type number.'; exit 2 ;;
  crash) echo 'compiler failed to start' >&2; exit 1 ;;
esac
`,
    )
    for (const executable of [
      join(bin, 'npm'),
      join(pkg, 'node_modules', '.bin', 'tsc'),
    ]) {
      chmodSync(executable, 0o755)
    }
    return spawnSync(
      'bash',
      [join(sdk, 'check-consumers.sh'), '-p', '3.0.4', 'consumer'],
      {
        cwd: root,
        encoding: 'utf8',
        env: {
          ...process.env,
          PATH: `${bin}:${process.env.PATH}`,
          SDK_FIXTURE: fixture,
          PREVIOUS: previous,
          CANDIDATE: candidate,
        },
      },
    )
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

test('clean consumers pass', () => {
  assert.equal(check('clean', 'clean').status, 0)
})

test('new diagnostics fail', () => {
  const result = check('clean', 'diagnostic')
  assert.equal(result.status, 1)
  assert.match(result.stdout, /FAIL/)
})

test('existing diagnostics and resolved diagnostics pass', () => {
  assert.equal(check('diagnostic', 'diagnostic').status, 0)
  assert.equal(check('diagnostic', 'clean').status, 0)
})

for (const side of ['previous', 'candidate']) {
  test(`${side} compiler failures fail the check`, () => {
    const result = check(
      side === 'previous' ? 'crash' : 'clean',
      side === 'candidate' ? 'crash' : 'clean',
    )
    assert.equal(result.status, 2)
    assert.match(result.stderr, /compiler failed to start/)
  })
}
