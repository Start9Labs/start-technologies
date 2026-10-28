import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO, fixture } from '../../../scripts/tests/fixtures.mjs'

const helper = './.github/workflows/compile-start-cli.yaml'
const hostTriple = 'x86_64-unknown-linux-musl'
const workflow = name =>
  yaml.load(readFileSync(join(REPO, '.github/workflows', name), 'utf8'))
const compile = workflow('compile-start-cli.yaml')
const cli = workflow('start-cli.yaml')

function evaluate(expression, context) {
  const fromJson = text => {
    const value = JSON.parse(text)
    return Array.isArray(value)
      ? new Proxy(value, {
          get: (array, key) =>
            array[key === 'true' ? 1 : key === 'false' ? 0 : key],
        })
      : value
  }
  const scope = {
    fromJson,
    contains: (value, item) => value.includes(item),
    format: (value, ...args) =>
      value.replace(/\{(\d+)\}/g, (_, index) => args[index]),
    cancelled: () => false,
    ...context,
  }
  return Function(
    ...Object.keys(scope),
    `return (${expression
      .replace(/^\s*\$\{\{|\}\}\s*$/g, '')
      .replaceAll('\n', ' ')
      .replace(/\.([a-zA-Z_]\w*(?:-\w+)+)/g, "['$1']")
      .trim()})`,
  )(...Object.values(scope))
}

function context(event, inputs = {}, ref = 'refs/heads/master') {
  return {
    github: {
      event_name: event,
      ref,
      event: { inputs, pull_request: { draft: false } },
    },
    vars: { STARTWRT_ALPHA_REGISTRY: '' },
    needs: { image: { result: 'success' } },
  }
}

test('CLI matrix delegates the same targets, environment and runner to one artifact compiler', () => {
  assert.equal(cli.jobs.compile.uses, helper)
  assert.equal(cli.jobs.compile.steps, undefined)
  assert.deepEqual(cli.jobs.deb.needs, ['compile'])
  assert.equal(cli.jobs.compile.with.environment, cli.env.ENVIRONMENT)
  assert.equal(cli.jobs.compile.with['nodejs-version'], cli.env.NODEJS_VERSION)
  assert.equal(cli.jobs.compile.with.triple, '${{ matrix.triple }}')
  const triples = {
    x86_64: hostTriple,
    'x86_64-apple': 'x86_64-apple-darwin',
    aarch64: 'aarch64-unknown-linux-musl',
    'aarch64-apple': 'aarch64-apple-darwin',
    riscv64: 'riscv64gc-unknown-linux-musl',
  }
  for (const [arch, triple] of Object.entries(triples))
    assert.deepEqual(
      evaluate(
        cli.jobs.compile.strategy.matrix.triple,
        context('workflow_dispatch', { arch }),
      ),
      [triple],
    )
  assert.deepEqual(
    evaluate(cli.jobs.compile.strategy.matrix.triple, context('pull_request')),
    Object.values(triples),
  )
  for (const [event, input, ref, expected] of [
    ['push', {}, 'refs/heads/master', ''],
    ['push', {}, 'refs/heads/integration/example', 'dev'],
    ['pull_request', {}, 'refs/pull/1/merge', 'dev'],
    ['workflow_dispatch', { environment: 'NONE' }, 'refs/heads/master', ''],
    [
      'workflow_dispatch',
      { environment: 'dev-unstable' },
      'refs/heads/master',
      'dev-unstable',
    ],
  ])
    assert.equal(
      evaluate(cli.jobs.compile.with.environment, context(event, input, ref)),
      expected,
    )
  for (const fast of [false, true]) {
    const inputs = { fast }
    assert.equal(
      evaluate(compile.jobs.compile['runs-on'], { inputs }),
      fast ? 'ubuntu-24.04-32-cores' : 'ubuntu-latest',
    )
    assert.equal(evaluate(compile.jobs.compile.steps[0].if, { inputs }), fast)
    assert.equal(
      evaluate(
        cli.jobs.compile.with.fast,
        context('workflow_dispatch', { runner: fast ? 'fast' : 'standard' }),
      ),
      fast,
    )
  }
})

test('artifact compiler builds the caller snapshot with the production make recipe', t => {
  assert.deepEqual(Object.keys(compile.on), ['workflow_call'])
  assert.equal(compile.concurrency, undefined)
  assert.equal(compile.jobs.compile.concurrency, undefined)
  const steps = compile.jobs.compile.steps
  assert.deepEqual(
    steps.find(step => step.uses === 'actions/checkout@v6'),
    {
      uses: 'actions/checkout@v6',
    },
  )
  assert.equal(
    steps.find(step => step.uses === './.github/actions/setup-build').with[
      'nodejs-version'
    ],
    '${{ inputs.nodejs-version }}',
  )
  assert.equal(compile.env.ENVIRONMENT, '${{ inputs.environment }}')
  assert.equal(compile.on.workflow_call.inputs.environment.default, '')
  const make = steps.find(step => step.name === 'Make')
  const upload = steps.find(step => step.uses === 'actions/upload-artifact@v7')
  assert.equal(upload.with.name, 'start-cli_${{ inputs.triple }}')
  assert.equal(
    upload.with.path,
    'target/${{ inputs.triple }}/release/start-cli',
  )
  assert.equal(upload.with['retention-days'], 14)
  const { root, write, run } = fixture(t)
  write('head-snapshot', 'current commit')
  const result = run(
    'bash',
    [
      '-e',
      '-c',
      `
make() {
  [ "$1" = start-cli ] && [ "$TARGET" = '${hostTriple}' ] || return 1
  mkdir -p "target/$TARGET/release"
  cp head-snapshot "target/$TARGET/release/start-cli"
}
${make.run.replaceAll('${{ inputs.triple }}', hostTriple)}
`,
    ],
    { encoding: 'utf8' },
  )
  assert.equal(result.status, 0, result.stderr)
  assert.equal(
    readFileSync(
      join(
        root,
        upload.with.path.replaceAll('${{ inputs.triple }}', hostTriple),
      ),
      'utf8',
    ),
    'current commit',
  )
})

for (const [file, product] of [
  ['startos-iso.yaml', 'start-os'],
  ['start-wrt.yaml', 'start-wrt'],
]) {
  const source = workflow(file)
  const publisher = source.jobs['publisher-cli']
  const deploy = source.jobs.deploy
  test(`${product} builds a host CLI only for eligible deployments after successful images`, () => {
    assert.equal(publisher.uses, helper)
    assert.deepEqual(publisher.needs, ['image'])
    assert.deepEqual(publisher.with, { triple: hostTriple })
    assert.equal(publisher.concurrency, undefined)
    assert.deepEqual(deploy.needs, ['image', 'publisher-cli'])
    const allowedChannel = product === 'start-os' ? 'beta' : 'release'
    const cases = [
      [context('pull_request'), false],
      [context('push', {}, 'refs/heads/integration/example'), false],
      [context('push'), product === 'start-os'],
      [
        {
          ...context('push'),
          vars: { STARTWRT_ALPHA_REGISTRY: 'https://alpha.example' },
        },
        true,
      ],
      [
        context('workflow_dispatch', {
          deploy: allowedChannel,
          platform: 'riscv64',
        }),
        true,
      ],
      [context('workflow_dispatch', { deploy: 'NONE' }), false],
      [context('workflow_dispatch'), false],
      [context('workflow_dispatch', { deploy: 'unexpected' }), false],
    ]
    if (product === 'start-os')
      cases.push([
        context('workflow_dispatch', { deploy: 'alpha', platform: 'aarch64' }),
        true,
      ])
    for (const [ctx, expected] of cases) {
      assert.equal(evaluate(publisher.if, ctx), expected, JSON.stringify(ctx))
      for (const result of ['failure', 'cancelled', 'skipped'])
        assert.equal(
          evaluate(publisher.if, { ...ctx, needs: { image: { result } } }),
          false,
        )
      assert.equal(
        evaluate(publisher.if, { ...ctx, cancelled: () => true }),
        false,
      )
    }
    for (const image of ['success', 'failure', 'cancelled', 'skipped'])
      for (const tool of ['success', 'failure', 'cancelled', 'skipped'])
        assert.equal(
          evaluate(deploy.if, {
            needs: {
              image: { result: image },
              'publisher-cli': { result: tool },
            },
          }),
          image === 'success' && tool === 'success',
        )
    assert.equal(
      evaluate(deploy.if, {
        cancelled: () => true,
        needs: {
          image: { result: 'success' },
          'publisher-cli': { result: 'success' },
        },
      }),
      false,
    )
  })

  test(`${product} installs the current-run artifact and capability-checks before all writes`, t => {
    const steps = deploy.steps
    const download = steps.find(step => step.name === 'Download publisher CLI')
    assert.equal(download.uses, 'actions/download-artifact@v8')
    assert.deepEqual(download.with, {
      name: `start-cli_${hostTriple}`,
      path: '${{ runner.temp }}/publisher-cli',
    })
    assert.equal(deploy['runs-on'], 'ubuntu-latest')
    const install = steps.find(step => step.name === 'Install start-cli')
    assert.equal(steps.indexOf(download) + 1, steps.indexOf(install))
    const commands = install.run.trim().split('\n')
    assert.equal(
      commands[1],
      `./scripts/manage-release.sh check-registry-notes-cli ${product}`,
    )
    assert.equal(commands.length, 2)
    const compiler = steps.findIndex(
      step => step.name === 'Compile release notes',
    )
    assert.ok(compiler < steps.indexOf(install))
    const writes = steps.filter(step =>
      /s3cmd (put|del)|registry os (version (add|remove)|asset add)/.test(
        step.run || '',
      ),
    )
    assert.ok(writes.length >= 3)
    for (const step of writes)
      assert.ok(steps.indexOf(install) < steps.indexOf(step))
    const { root, write, run } = fixture(t)
    write(
      'runner/publisher-cli/start-cli',
      '#!/bin/bash\nprintf "same-commit-cli\\n"\n',
    )
    write(
      'scripts/manage-release.sh',
      `#!/bin/bash\n[ "$1 $2" = 'check-registry-notes-cli ${product}' ] && [ "$(start-cli)" = same-commit-cli ]\n`,
    )
    const result = run(
      'bash',
      [
        '-e',
        '-c',
        `
sudo() { [ "$1 $2 $3 $4" = 'install -m 755 ${root}/runner/publisher-cli/start-cli' ] && [ "$5" = /usr/local/bin/start-cli ] || return 1; install -m 755 "$4" ./start-cli; }
chmod +x scripts/manage-release.sh
${install.run}
`,
      ],
      {
        encoding: 'utf8',
        env: {
          ...process.env,
          RUNNER_TEMP: join(root, 'runner'),
          PATH: `${root}:${process.env.PATH}`,
        },
      },
    )
    assert.equal(result.status, 0, result.stderr)
    for (const step of [
      install,
      ...compile.jobs.compile.steps.filter(step => step.run),
    ]) {
      const syntax = run('bash', ['-n'], {
        input: step.run.replaceAll('${{ inputs.triple }}', hostTriple),
        encoding: 'utf8',
      })
      assert.equal(syntax.status, 0, syntax.stderr)
    }
  })
}

test('release-note catalog changes trigger image assembly without rebuilding images for binary-only paths', t => {
  const { env, write, run } = fixture(t)
  const filter = workflow('startos-iso.yaml').jobs.changes.steps.find(
    step => step.id === 'filter',
  )
  const shell = filter.run
    .replaceAll('${{ github.repository }}', 'Start9Labs/start-technologies')
    .replaceAll('${{ github.event.pull_request.number }}', '1')
  for (const [path, expected] of [
    ['shared-libs/crates/start-core/locales/release-notes.json', true],
    ['shared-libs/crates/start-core/locales/i18n.yaml', false],
    ['projects/start-os/changelog/patch-security-fix.pl_PL.md', true],
  ]) {
    const output = write('output', '')
    const result = run(
      'bash',
      ['-e', '-c', `gh() { printf '%s\\n' "$CHANGED_FILE"; }\n${shell}`],
      {
        encoding: 'utf8',
        env: {
          ...env,
          BASE_REF: 'master',
          HEAD_REF: 'fix/notes',
          HEAD_REPO: 'Start9Labs/start-technologies',
          CHANGED_FILE: path,
          GITHUB_OUTPUT: output,
        },
      },
    )
    assert.equal(result.status, 0, result.stderr)
    assert.equal(readFileSync(output, 'utf8'), `image=${expected}\n`)
  }
})

test('artifact consumers track shared inputs without CLI-only image rebuilds', () => {
  for (const [file, source] of [
    ['start-cli.yaml', cli],
    ['startos-iso.yaml', workflow('startos-iso.yaml')],
    ['start-wrt.yaml', workflow('start-wrt.yaml')],
  ])
    for (const event of ['push', 'pull_request']) {
      assert.ok(
        source.on[event].paths.includes(
          '.github/workflows/compile-start-cli.yaml',
        ),
      )
      assert.equal(
        source.on[event].paths.includes('projects/start-cli/**'),
        file === 'start-cli.yaml',
      )
      assert.ok(source.on[event].paths.includes(`.github/workflows/${file}`))
      assert.ok(
        source.on[event].paths.includes('shared-libs/crates/**') ||
          source.on[event].paths.includes('shared-libs/**'),
      )
    }
})
