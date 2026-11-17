import assert from 'node:assert/strict'
import { chmodSync, readFileSync, utimesSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO, fixture } from './fixtures.mjs'

const aggregateRule = readFileSync(join(REPO, 'Makefile'), 'utf8').match(
  /^rust-products:[^\n]*\n(?:\t[^\n]*\n)+/m,
)[0]

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
    f.write(
      'build/builder-alias.sh',
      `source ${JSON.stringify(join(REPO, 'build/builder-alias.sh'))}\nalias rust-zig-builder='true'\n`,
    )
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
