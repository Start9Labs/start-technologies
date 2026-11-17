import assert from 'node:assert/strict'
import { chmodSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO, fixture } from '../../../scripts/tests/fixtures.mjs'

const core = 'shared-libs/crates/start-core'
const builderImage = 'example.invalid/builder:test'

function copy(f, path) {
  return f.write(path, readFileSync(join(REPO, path)))
}

function executable(f, path, source) {
  chmodSync(f.write(path, `#!/bin/bash\nset -eu\n${source}`), 0o755)
}

// Runs a containerized build script against a docker stub that records each `docker` argv.
function builderScripts(t, scripts) {
  const f = fixture(t)
  for (const path of [
    'build/builder-alias.sh',
    'build/rust-builder/image.sh',
    'build/rust-builder/repository',
    'build/rust-builder/reference',
    ...scripts,
  ]) {
    chmodSync(copy(f, path), 0o755)
  }
  f.write('projects/start-cli/man/.keep', '')
  f.write(`${core}/bindings/.keep`, '')
  f.nodeExecutable(
    'docker',
    `import { appendFileSync } from 'node:fs'
appendFileSync(process.env.DOCKER_LOG, JSON.stringify(process.argv.slice(2)) + '\\n')`,
  )
  const log = join(f.root, 'docker.log')
  return (script, profile, environment) => {
    f.write('docker.log', '')
    const env = {
      ...f.env,
      PATH: `${f.root}:${f.env.PATH}`,
      DOCKER_LOG: log,
      RUST_BUILDER_IMAGE: builderImage,
      ARCH: 'arm64',
      ENVIRONMENT: environment,
    }
    delete env.PROFILE
    if (profile !== undefined) env.PROFILE = profile
    const result = f.run('bash', [script], { env, encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    return readFileSync(log, 'utf8').trim().split('\n').map(JSON.parse)
  }
}

test('the bindings generator uses the generated profile and preserves explicit build settings', t => {
  const run = builderScripts(t, [
    `${core}/build/generator-env.sh`,
    `${core}/build/build-ts.sh`,
  ])
  for (const [profile, expected, environment] of [
    [undefined, 'generated', 'dev-unstable'],
    ['debug', 'dev', 'dev-unstable'],
    ['dev', 'dev', 'dev-console-unstable'],
    ['release', 'release', 'dev-unstable'],
    ['generated', 'generated', 'dev-unstable'],
    ['unknown', 'dev', 'dev-unstable'],
  ]) {
    const [args] = run(`${core}/build/build-ts.sh`, profile, environment)
    assert.equal(args[args.indexOf('--profile') + 1], expected)
    assert.ok(args.includes(builderImage))
    assert.ok(args.includes('PKG_CONFIG_SYSROOT_DIR=/opt/sysroot/aarch64'))
    assert.ok(
      args.includes(
        `RUSTFLAGS=${environment.includes('console') ? '--cfg tokio_unstable' : ''}`,
      ),
    )
    assert.equal(
      args[args.indexOf('--features') + 1],
      `ts,${environment.replaceAll('-', ',')}`,
    )
    assert.ok(args.includes('--locked'))
  }
})

test('man pages build with exactly the unit-test harness settings, so the test build is reused', t => {
  const run = builderScripts(t, [
    `${core}/build/test-env.sh`,
    `${core}/run-tests.sh`,
    `${core}/build/build-manpage.sh`,
  ])
  const cargo = (args, end) =>
    args.slice(args.indexOf(builderImage) + 1, args.indexOf(end))
  for (const [profile, flags] of [
    [undefined, ['--profile', 'unit-test']],
    ['unit-test', ['--profile', 'unit-test']],
    ['release', ['--release']],
    ['dev', []],
    ['debug', []],
    ['unknown', []],
  ]) {
    for (const environment of ['dev-unstable', 'dev-console-unstable']) {
      const [tests, conversion] = run(
        `${core}/run-tests.sh`,
        profile,
        environment,
      )
      const [manpages] = run(
        `${core}/build/build-manpage.sh`,
        profile,
        environment,
      )
      const shared = cargo(tests, '--')
      assert.deepEqual(cargo(manpages, 'export_manpage_'), shared)
      assert.deepEqual(
        manpages.slice(0, manpages.indexOf(builderImage)),
        tests.slice(0, tests.indexOf(builderImage)),
      )
      assert.ok(
        conversion
          .at(-1)
          .endsWith(
            ` ${shared.join(' ')} backup::target::conversion::tests::worker_ -- --ignored`,
          ),
        conversion.at(-1),
      )
      assert.deepEqual(shared.slice(0, 2), ['cargo', 'test'])
      assert.deepEqual(
        shared.filter(
          (arg, i) =>
            arg === '--release' ||
            arg === '--profile' ||
            shared[i - 1] === '--profile',
        ),
        flags,
      )
      assert.ok(
        shared.includes(
          `--features=test,ts,${environment.replaceAll('-', ',')}`,
        ),
      )
      assert.ok(shared.includes('--locked'))
      assert.ok(
        tests.includes(
          `RUSTFLAGS=${environment.includes('console') ? '--cfg tokio_unstable' : ''}`,
        ),
      )
    }
  }
})

// Builds a probe crate under the workspace's Cargo profiles; returns each crate's rustc command.
function profileProbe(t, profile) {
  const f = fixture(t)
  const manifest = readFileSync(join(REPO, 'Cargo.toml'), 'utf8')
  const start = manifest.indexOf('\n[profile.')
  assert.notEqual(start, -1)
  f.write(
    'probe/Cargo.toml',
    `[package]\nname = "profile-probe"\nversion = "0.0.0"\nedition = "2024"\n\n[dependencies]\nprobe-dependency = { path = "../dependency" }\n${manifest.slice(start)}`,
  )
  f.write(
    'probe/src/lib.rs',
    'pub fn probe(value: u64) -> u64 { probe_dependency::step(value) }\n',
  )
  f.write(
    'dependency/Cargo.toml',
    '[package]\nname = "probe-dependency"\nversion = "0.0.0"\nedition = "2024"\n',
  )
  f.write(
    'dependency/src/lib.rs',
    'pub fn step(value: u64) -> u64 { value.wrapping_add(1) }\n',
  )
  const env = {
    ...f.env,
    CARGO_TARGET_DIR: join(f.root, 'target'),
    RUSTFLAGS: '',
    CARGO_ENCODED_RUSTFLAGS: '',
  }
  delete env.CARGO_INCREMENTAL
  const result = f.run(
    'cargo',
    ['build', '--lib', '--profile', profile, '--verbose', '--offline'],
    { cwd: join(f.root, 'probe'), env, encoding: 'utf8' },
  )
  assert.equal(result.status, 0, result.stderr)
  return crate => {
    const command = result.stderr
      .split('\n')
      .find(line => line.includes(`--crate-name ${crate} `))
    assert.ok(command, result.stderr)
    assert.doesNotMatch(command, /-C debuginfo=/)
    assert.doesNotMatch(command, /-C incremental=/)
    return command
  }
}

test('the generated Cargo profile uses level-1 optimization without debug data or incremental state', t => {
  const command = profileProbe(t, 'generated')
  assert.match(command('profile_probe'), /-C opt-level=1\b/)
})

test('the unit-test Cargo profile leaves workspace code unoptimized and optimizes dependencies', t => {
  const command = profileProbe(t, 'unit-test')
  assert.doesNotMatch(command('profile_probe'), /-C opt-level=/)
  assert.match(command('probe_dependency'), /-C opt-level=3\b/)
})

for (const [name, content, failBindings] of [
  ['clean outputs pass', 'initial', false],
  ['all drift checks run without rebuilding bindings', 'updated', false],
  [
    'a generator failure fails the job without running dependent generation',
    'updated',
    true,
  ],
]) {
  test(`generated-artifact checks: ${name}`, t => {
    const f = fixture(t)
    const workflow = yaml.load(
      readFileSync(join(REPO, '.github/workflows/test.yaml'), 'utf8'),
    )
    const coreSteps = workflow.jobs['start-core'].steps
    const manpages = coreSteps.filter(step =>
      step.run?.includes('manpages-check'),
    )
    assert.equal(manpages.length, 1)
    assert.equal(manpages[0].if, '!cancelled()')
    assert.ok(
      coreSteps.indexOf(manpages[0]) >
        coreSteps.findIndex(step => step.run === 'make start-core-test'),
    )
    const generated = workflow.jobs.generated.steps.filter(step =>
      step.run?.includes('start-core-ts-bindings-check'),
    )
    assert.equal(generated.length, 1)
    f.write(
      'Makefile',
      `GIT_HASH_FILE := $(shell ./build/env/check-git-hash.sh)\nCORE_SRC := $(GIT_HASH_FILE)\nENVIRONMENT_FILE :=\ninclude ${join(REPO, core, 'build.mk')}\n`,
    )
    chmodSync(copy(f, 'build/env/check-git-hash.sh'), 0o755)
    f.write('Cargo.toml', '')
    f.write('Cargo.lock', '')
    f.write('projects/start-cli/man/probe.1', 'initial\n')
    f.write(
      'shared-libs/ts-modules/start-core/lib/osBindings/index.ts',
      'initial\n',
    )
    f.write(`${core}/mcp-tools.json`, 'initial\n')
    executable(
      f,
      `${core}/build/build-manpage.sh`,
      `
echo man >> "$GENERATION_LOG"
printf '%s\\n' "$GENERATED_CONTENT" > projects/start-cli/man/probe.1
`,
    )
    executable(
      f,
      `${core}/build/build-ts.sh`,
      `
echo bindings >> "$GENERATION_LOG"
if [ "$FAIL_BINDINGS" = true ]; then exit 7; fi
mkdir -p ${core}/bindings
printf '%s\\n' "$GENERATED_CONTENT" > ${core}/bindings/index.ts
`,
    )
    f.nodeExecutable(
      'npm',
      `import { appendFileSync, writeFileSync } from 'node:fs'
if (process.argv.includes('tsx')) {
  appendFileSync(process.env.GENERATION_LOG, 'mcp\\n')
  writeFileSync('${core}/mcp-tools.json', process.env.GENERATED_CONTENT + '\\n')
}`,
    )
    f.git('init', '-q')
    f.git('config', 'user.name', 'CI fixture')
    f.git('config', 'user.email', 'ci-fixture@example.invalid')
    f.git('config', 'commit.gpgsign', 'false')
    f.git('add', '.')
    f.git('commit', '-qm', 'fixture')
    const [man, bindings] = [manpages[0], generated[0]].map(step =>
      f.run('bash', ['-e', '-c', step.run], {
        env: {
          ...f.env,
          PATH: `${f.root}:${f.env.PATH}`,
          GENERATION_LOG: join(f.root, 'generation.log'),
          GENERATED_CONTENT: content,
          FAIL_BINDINGS: String(failBindings),
          GIT_BRANCH_AS_HASH: '0',
        },
        encoding: 'utf8',
      }),
    )
    for (const result of [man, bindings])
      assert.equal(
        result.status === 0,
        content === 'initial',
        result.stdout + result.stderr,
      )
    assert.equal(
      readFileSync(join(f.root, 'generation.log'), 'utf8'),
      failBindings ? 'man\nbindings\n' : 'man\nbindings\nmcp\n',
    )
    if (content !== 'initial')
      assert.match(man.stdout, /Man pages are out of date/)
    if (!failBindings && content !== 'initial') {
      assert.match(bindings.stdout, /TS bindings are out of date/)
      assert.match(bindings.stdout, /MCP tool catalog is out of date/)
    }
  })
}
