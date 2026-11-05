import assert from 'node:assert/strict'
import { chmodSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO, fixture } from '../../../scripts/tests/fixtures.mjs'

const core = 'shared-libs/crates/start-core'
const scripts = ['build-manpage.sh', 'build-ts.sh']

function copy(f, path) {
  return f.write(path, readFileSync(join(REPO, path)))
}

function executable(f, path, source) {
  chmodSync(f.write(path, `#!/bin/bash\nset -eu\n${source}`), 0o755)
}

test('both generators share the optimized profile and preserve explicit build settings', t => {
  const f = fixture(t)
  for (const path of [
    'build/builder-alias.sh',
    'build/rust-builder/image.sh',
    'build/rust-builder/repository',
    'build/rust-builder/reference',
    `${core}/build/generator-env.sh`,
    ...scripts.map(script => `${core}/build/${script}`),
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
  for (const [profile, expected, environment] of [
    [undefined, 'generated', 'dev-unstable'],
    ['debug', 'dev', 'dev-unstable'],
    ['dev', 'dev', 'dev-console-unstable'],
    ['release', 'release', 'dev-unstable'],
    ['generated', 'generated', 'dev-unstable'],
    ['unknown', 'dev', 'dev-unstable'],
  ]) {
    for (const script of scripts) {
      f.write('docker.log', '')
      const env = {
        ...f.env,
        PATH: `${f.root}:${f.env.PATH}`,
        DOCKER_LOG: log,
        RUST_BUILDER_IMAGE: 'example.invalid/builder:test',
        ARCH: 'arm64',
        ENVIRONMENT: environment,
      }
      delete env.PROFILE
      if (profile !== undefined) env.PROFILE = profile
      const result = f.run('bash', [`${core}/build/${script}`], {
        env,
        encoding: 'utf8',
      })
      assert.equal(result.status, 0, result.stderr)
      const [args] = readFileSync(log, 'utf8')
        .trim()
        .split('\n')
        .map(JSON.parse)
      assert.equal(args[args.indexOf('--profile') + 1], expected)
      assert.ok(args.includes('example.invalid/builder:test'))
      assert.ok(args.includes('PKG_CONFIG_SYSROOT_DIR=/opt/sysroot/aarch64'))
      assert.ok(
        args.includes(
          `RUSTFLAGS=${environment.includes('console') ? '--cfg tokio_unstable' : ''}`,
        ),
      )
      assert.equal(
        args[args.indexOf('--features') + 1],
        `${script === 'build-ts.sh' ? 'ts' : 'test'},${environment.replaceAll('-', ',')}`,
      )
      assert.ok(args.includes('--locked'))
    }
  }
})

test('the generated Cargo profile retains level-3 optimization without debug data', t => {
  const f = fixture(t)
  const manifest = readFileSync(join(REPO, 'Cargo.toml'), 'utf8')
  const start = manifest.indexOf('\n[profile.')
  assert.notEqual(start, -1)
  f.write(
    'Cargo.toml',
    `[package]\nname = "generated-profile-probe"\nversion = "0.0.0"\nedition = "2024"\n${manifest.slice(start)}`,
  )
  f.write(
    'src/lib.rs',
    'pub fn probe(value: u64) -> u64 { value.wrapping_add(1) }\n',
  )
  const result = f.run(
    'cargo',
    ['build', '--lib', '--profile', 'generated', '--verbose', '--offline'],
    {
      env: {
        ...f.env,
        CARGO_TARGET_DIR: join(f.root, 'target'),
        RUSTFLAGS: '',
        CARGO_ENCODED_RUSTFLAGS: '',
      },
      encoding: 'utf8',
    },
  )
  assert.equal(result.status, 0, result.stderr)
  const command = result.stderr
    .split('\n')
    .find(line => line.includes('--crate-name generated_profile_probe'))
  assert.ok(command, result.stderr)
  assert.match(command, /-C opt-level=3\b/)
  assert.doesNotMatch(command, /-C debuginfo=/)
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
  test(`generated-artifact workflow: ${name}`, t => {
    const f = fixture(t)
    const workflow = yaml.load(
      readFileSync(join(REPO, '.github/workflows/test.yaml'), 'utf8'),
    )
    const steps = workflow.jobs.generated.steps.filter(step =>
      step.run?.includes('generated-artifacts-check'),
    )
    assert.equal(steps.length, 1)
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
    const result = f.run('bash', ['-e', '-c', steps[0].run], {
      env: {
        ...f.env,
        PATH: `${f.root}:${f.env.PATH}`,
        GENERATION_LOG: join(f.root, 'generation.log'),
        GENERATED_CONTENT: content,
        FAIL_BINDINGS: String(failBindings),
        GIT_BRANCH_AS_HASH: '0',
      },
      encoding: 'utf8',
    })
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
      assert.match(result.stdout, /Man pages are out of date/)
    if (!failBindings && content !== 'initial') {
      assert.match(result.stdout, /TS bindings are out of date/)
      assert.match(result.stdout, /MCP tool catalog is out of date/)
    }
  })
}
