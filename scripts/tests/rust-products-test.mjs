import assert from 'node:assert/strict'
import {
  chmodSync,
  existsSync,
  readFileSync,
  rmSync,
  utimesSync,
} from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO, fixture } from './fixtures.mjs'

const aggregateRule = readFileSync(join(REPO, 'Makefile'), 'utf8').match(
  /^rust-products:[^\n]*\n(?:\t[^\n]*\n)+/m,
)[0]
const commonMk = readFileSync(join(REPO, 'build/common.mk'), 'utf8')
const archRule = commonMk.match(/^ARCH :=[^\n]*$/m)[0]
const rustArchRule = commonMk.match(/^RUST_ARCH :=[^\n]*$/m)[0]

for (const profile of ['release', 'debug']) {
  test(`rust-products ${profile} refreshes release start-container`, t => {
    const f = fixture(t)
    f.write('version', '0.4.1')
    f.write('environment', '')
    f.write('source', 'changed source')
    f.write('projects/start-os/web/patchdb-ui-seed.json', '{}')
    f.write('projects/start-os/build/lib/firmware.json', '[]')
    const target = 'target/x86_64-unknown-linux-musl'
    // Older than every input: file times are too coarse to order writes made in one test.
    utimesSync(
      f.write(`${target}/release/start-container`, 'stale container'),
      1000,
      1000,
    )
    const builder = f.write(
      'build/build-rust-bins.sh',
      `#!/bin/bash
set -eu
printf '%s %s\\n' "$PROFILE" "$*" >> builds.log
for pair; do
    output="target/x86_64-unknown-linux-musl/$PROFILE/\${pair#*:}"
    mkdir -p "$(dirname "$output")"
    printf 'built-%s' "$PROFILE" > "$output"
done
`,
    )
    chmodSync(builder, 0o755)
    f.write(
      'Makefile',
      `PROFILE := ${profile}
ARCH := x86_64
RUST_ARCH := x86_64
PLATFORM := x86_64
VERSION_FILE := version
ENVIRONMENT_FILE := environment
CORE_SRC := source
RUST_BIN_BUILDER_INPUTS := build/build-rust-bins.sh
RUST_PRODUCTS := start-os
include ${join(REPO, 'projects/start-os/build.mk')}
.PHONY: rust-products
${aggregateRule}`,
    )
    const result = f.run('make', ['rust-products'], { encoding: 'utf8' })
    assert.equal(result.status, 0, result.stdout + result.stderr)
    assert.equal(
      readFileSync(join(f.root, target, 'release/start-container'), 'utf8'),
      'built-release',
    )
    assert.equal(
      readFileSync(join(f.root, target, profile, 'startbox'), 'utf8'),
      `built-${profile}`,
    )
    const builds = readFileSync(join(f.root, 'builds.log'), 'utf8')
      .trim()
      .split('\n')
    assert.equal(builds.length, profile === 'release' ? 1 : 2)
  })
}

for (const profile of ['release', 'debug', 'dev']) {
  test(`CLI install uses the compiled output for ${profile}`, t => {
    const f = fixture(t)
    for (const path of [
      'build/build-rust-bins.sh',
      'build/rust-build-profile.sh',
      'projects/start-cli/build/build-cli.sh',
    ]) {
      chmodSync(f.write(path, readFileSync(join(REPO, path))), 0o755)
    }
    f.write('build/builder-alias.sh', "alias rust-zig-builder='true'\n")
    const target = 'x86_64-unknown-linux-musl'
    for (const outputProfile of ['release', 'debug']) {
      f.write(`target/${target}/${outputProfile}/start-cli`, outputProfile)
    }
    f.write('home/.cargo/bin/.keep', '')
    const result = f.run(
      'bash',
      ['projects/start-cli/build/build-cli.sh', '--install'],
      {
        env: {
          ...f.env,
          HOME: join(f.root, 'home'),
          TARGET: target,
          ARCH: 'x86_64',
          ENVIRONMENT: '',
          PROFILE: profile,
        },
        encoding: 'utf8',
      },
    )
    assert.equal(result.status, 0, result.stdout + result.stderr)
    assert.equal(
      readFileSync(join(f.root, 'home/.cargo/bin/start-cli'), 'utf8'),
      profile === 'release' ? 'release' : 'debug',
    )
  })
}

function setupCompiledFixture(t, platform) {
  const f = fixture(t)
  f.write('version', '0.4.1')
  f.write('environment', '')
  f.write('git-hash', 'deadbeef')
  f.write('platform', platform)
  f.write('source', 'src')
  f.write('projects/start-os/web/patchdb-ui-seed.json', '{}')
  f.write('projects/start-os/build/lib/firmware.json', '[]')
  f.write('projects/start-os/container-runtime/package.json', '{}')
  f.write('projects/start-os/container-runtime/package-lock.json', '{}')
  f.write('projects/start-os/container-runtime/tsconfig.json', '{}')
  f.write('projects/start-os/container-runtime/src/index.ts', '')
  f.write('projects/start-os/container-runtime/install-dist-deps.sh', '')
  f.write('projects/start-sdk/dist/package.json', '{}')
  f.write('shared-libs/ts-modules/start-core/dist/package.json', '{}')
  f.write('shared-libs/crates/pi-beep/src/main.rs', 'fn main() {}\n')
  f.write('Cargo.toml', '')
  f.write('Cargo.lock', '')
  f.write('build/bash-completion', '')
  for (const path of [
    'projects/start-os/container-runtime/node_modules/.package-lock.json',
    'projects/start-os/container-runtime/dist/index.js',
    'projects/start-os/container-runtime/dist/node_modules/.package-lock.json',
  ]) {
    utimesSync(f.write(path, 'ok'), 2000000000, 2000000000)
  }
  const makeStub = (path, body) => chmodSync(f.write(path, body), 0o755)
  const writeTargetBin = binsExpr => `#!/bin/bash
set -eu
rust_arch="$ARCH"
[ "$rust_arch" = riscv64 ] && rust_arch=riscv64gc
for name in ${binsExpr}; do
  out="target/\${rust_arch}-unknown-linux-musl/\${PROFILE:-release}/\${name#*:}"
  mkdir -p "$(dirname "$out")"
  printf '%s' "\${name#*:}" > "$out"
done
`
  makeStub('build/build-rust-bins.sh', writeTargetBin('"$@"'))
  makeStub(
    'projects/start-os/backup-fs/build-backup-fs.sh',
    writeTargetBin('startos-backup-fs'),
  )
  makeStub(
    'shared-libs/crates/pi-beep/build-pi-beep.sh',
    `#!/bin/bash
set -eu
printf '%s %s\\n' "$ARCH" "$PROFILE" >> pi-beep-builds.log
if [ "\${FAIL_PI_BEEP:-0}" = 1 ]; then
  echo "error[E0308]: mismatched types" >&2
  exit 1
fi
out="target/\${ARCH}-unknown-linux-musl/\${PROFILE}/pi-beep"
mkdir -p "$(dirname "$out")"
printf 'pi-beep' > "$out"
`,
  )
  f.write(
    'Makefile',
    `PROFILE := release
PLATFORM := ${platform}
${archRule}
${rustArchRule}
PLATFORM_FILE := platform
ENVIRONMENT_FILE := environment
GIT_HASH_FILE := git-hash
VERSION_FILE := version
CORE_SRC := source
RUST_BIN_BUILDER_INPUTS := build/build-rust-bins.sh
RUST_ZIG_BUILDER_INPUTS :=
ls-files = $(wildcard $(1)/* $(1)/*/*)
mkdir = mkdir -p $1
rm = rm -rf $1
cp = cp -r $1 $2
ln = ln -sf $1 $2
include ${join(REPO, 'projects/start-os/build.mk')}
show-startos-pi-beep:
\t@printf '%s\\n' $(filter %/pi-beep,$(STARTOS_TARGETS))
`,
  )
  return f
}

const piBeepPath = 'target/aarch64-unknown-linux-musl/release/pi-beep'

for (const [platform, arch, expectsPiBeep] of [
  ['aarch64', 'aarch64', true],
  ['raspberrypi', 'aarch64', true],
  ['x86_64', 'x86_64', false],
  ['riscv64', 'riscv64', false],
]) {
  test(`compiled-${arch}.tar.gz for ${platform} ${expectsPiBeep ? 'compiles and packs' : 'excludes'} pi-beep`, t => {
    const f = setupCompiledFixture(t, platform)

    const built = f.run('make', [`compiled-${arch}.tar.gz`], {
      encoding: 'utf8',
    })
    assert.equal(built.status, 0, built.stdout + built.stderr)

    const listing = f.run('tar', ['-tzf', `compiled-${arch}.tar.gz`], {
      encoding: 'utf8',
    })
    assert.equal(listing.status, 0, listing.stderr)
    const entries = listing.stdout.trim().split('\n')
    assert.equal(entries.includes(piBeepPath), expectsPiBeep)

    const buildLog = join(f.root, 'pi-beep-builds.log')
    assert.equal(existsSync(buildLog), expectsPiBeep)
    if (expectsPiBeep) {
      assert.equal(readFileSync(buildLog, 'utf8').trim(), 'aarch64 release')
    }

    const startosPiBeep = f.run('make', ['-s', 'show-startos-pi-beep'], {
      encoding: 'utf8',
    })
    assert.equal(startosPiBeep.status, 0, startosPiBeep.stderr)
    const occurrences = startosPiBeep.stdout.trim().split('\n').filter(Boolean)
    assert.deepEqual(occurrences, expectsPiBeep ? [piBeepPath] : [])
  })
}

for (const [platform, expectsInstalled] of [
  ['raspberrypi', true],
  ['aarch64', false],
]) {
  test(`unpacked compiled-aarch64.tar.gz skips pi-beep rebuild and ${expectsInstalled ? 'installs' : 'omits'} it for ${platform}`, t => {
    const f = setupCompiledFixture(t, platform)
    const built = f.run('make', ['compiled-aarch64.tar.gz'], {
      encoding: 'utf8',
    })
    assert.equal(built.status, 0, built.stdout + built.stderr)

    rmSync(join(f.root, piBeepPath))
    const unpacked = f.run('tar', ['-xzf', 'compiled-aarch64.tar.gz'], {
      encoding: 'utf8',
    })
    assert.equal(unpacked.status, 0, unpacked.stderr)
    assert.ok(existsSync(join(f.root, piBeepPath)))
    // Predates the pi-beep sources.
    utimesSync(join(f.root, piBeepPath), 1000, 1000)

    const touched = f.run('make', ['-t', 'compiled-aarch64.tar.gz'], {
      encoding: 'utf8',
    })
    assert.equal(touched.status, 0, touched.stderr)

    const stage = join(f.root, 'stage')
    const installPlan = f.run(
      'make',
      [
        '-n',
        'start-os-install',
        `DESTDIR=${stage}`,
        'STARTOS_TARGETS=$(COMPILED_TARGETS)',
      ],
      { encoding: 'utf8' },
    )
    assert.equal(installPlan.status, 0, installPlan.stderr)
    assert.equal(installPlan.stdout.includes('build-pi-beep.sh'), false)

    const installCmd = installPlan.stdout
      .split('\n')
      .find(line => line.includes('/usr/bin/pi-beep'))
    assert.ok(installCmd)
    const ran = f.run(
      'bash',
      ['-euc', `mkdir -p "${stage}/usr/bin"; ${installCmd}`],
      { encoding: 'utf8' },
    )
    assert.equal(ran.status, 0, ran.stderr)
    assert.equal(existsSync(join(stage, 'usr/bin/pi-beep')), expectsInstalled)
  })
}

test('compiled-aarch64.tar.gz fails when pi-beep fails to compile', t => {
  const f = setupCompiledFixture(t, 'aarch64')
  const failed = f.run('make', ['compiled-aarch64.tar.gz'], {
    env: { ...f.env, FAIL_PI_BEEP: '1' },
    encoding: 'utf8',
  })
  assert.notEqual(failed.status, 0)
  assert.match(failed.stderr, /error\[E0308\]/)
  assert.equal(existsSync(join(f.root, 'compiled-aarch64.tar.gz')), false)
})
