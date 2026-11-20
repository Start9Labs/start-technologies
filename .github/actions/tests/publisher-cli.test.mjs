import assert from 'node:assert/strict'
import { chmodSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO, fixture } from '../../../scripts/tests/fixtures.mjs'

const workflow = name =>
  yaml.load(readFileSync(join(REPO, '.github/workflows', name), 'utf8'))
const products = workflow('build-products.yaml')
const iso = workflow('startos-iso.yaml')
const step = (job, name) => {
  const found = job.steps.find(step => step.name === name)
  assert.ok(found, name)
  return found
}

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

// Expands each `${{ expression }}` in a workflow string.
const interpolate = (template, context) =>
  template.replace(/\$\{\{(.*?)\}\}/gs, (_, expression) =>
    evaluate(expression, context),
  )

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

const SHA = 'c'.repeat(40)
const ALL_PLATFORMS = [
  'x86_64',
  'x86_64-nonfree',
  'x86_64-nvidia',
  'aarch64',
  'aarch64-nonfree',
  'aarch64-nvidia',
  'raspberrypi',
  'riscv64',
  'riscv64-nonfree',
]

function successfulPush(overrides = {}) {
  return {
    id: 1,
    run_number: 1,
    event: 'push',
    head_branch: 'master',
    head_sha: 'd'.repeat(40),
    head_repository: { full_name: 'Start9Labs/start-technologies' },
    status: 'completed',
    conclusion: 'success',
    ...overrides,
  }
}

function runStep(
  t,
  script,
  env,
  { files = [], fetches = true, pages = [[successfulPush()]], diffs = {} } = {},
) {
  const f = fixture(t)
  for (const script of [
    '.github/scripts/changed-products.mjs',
    '.github/scripts/classify-event.sh',
  ])
    chmodSync(f.write(script, readFileSync(join(REPO, script))), 0o755)
  f.write(
    'files.json',
    JSON.stringify(
      files.map(file => (typeof file === 'string' ? { filename: file } : file)),
    ),
  )
  f.write('pages.json', JSON.stringify(pages))
  f.write('diffs.json', JSON.stringify(diffs))
  f.write('gh.log', '')
  f.nodeExecutable(
    'gh',
    `import { appendFileSync, readFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
const args = process.argv.slice(2)
appendFileSync(${JSON.stringify(join(f.root, 'gh.log'))}, JSON.stringify(args) + '\\n')
if (args[0] === 'api') {
  let response
  if (args[1].includes('/actions/workflows/') && args[1].includes('/runs?')) {
    const page = Number(new URL('https://x/' + args[1]).searchParams.get('page'))
    const pages = JSON.parse(readFileSync(${JSON.stringify(join(f.root, 'pages.json'))}))
    response = { workflow_runs: pages[page - 1] ?? [] }
  } else response = JSON.parse(readFileSync(${JSON.stringify(join(f.root, 'files.json'))}))
  const result = spawnSync('jq', ['-r', args[args.indexOf('--jq') + 1]], { input: JSON.stringify(response), encoding: 'utf8' })
  process.stdout.write(result.stdout)
  process.stderr.write(result.stderr)
  process.exit(result.status)
}
`,
  )
  f.nodeExecutable(
    'git',
    `import { readFileSync } from 'node:fs'
const args = process.argv.slice(2)
if (args[0] === 'fetch') process.exit(${fetches ? 0 : 1})
if (args[0] === 'diff') {
  const diffs = JSON.parse(readFileSync(${JSON.stringify(join(f.root, 'diffs.json'))}))
  const files = JSON.parse(readFileSync(${JSON.stringify(join(f.root, 'files.json'))}))
  process.stdout.write((diffs[args.at(-2)] ?? files.map(file => file.filename)).join('\\n') + '\\n')
}
`,
  )
  const output = f.write('output', '')
  const result = f.run('bash', ['-e', '-c', script], {
    env: {
      ...f.env,
      PATH: `${f.root}:${f.env.PATH}`,
      GITHUB_OUTPUT: output,
      GITHUB_REPOSITORY: 'Start9Labs/start-technologies',
      GITHUB_SHA: SHA,
      GITHUB_RUN_ID: '42',
      GITHUB_REF_NAME: 'master',
      ...env,
    },
    encoding: 'utf8',
  })
  assert.equal(result.status, 0, result.stdout + result.stderr)
  const outputs = Object.fromEntries(
    readFileSync(output, 'utf8')
      .trim()
      .split('\n')
      .map(line => line.split(/=(.*)/s).slice(0, 2)),
  )
  const gh = readFileSync(join(f.root, 'gh.log'), 'utf8')
    .trim()
    .split('\n')
    .filter(Boolean)
    .map(line => JSON.parse(line))
  return { outputs, gh }
}

const declared = found =>
  Object.fromEntries(Object.keys(found.env ?? {}).map(key => [key, '']))
const classifyStep = step(products.jobs.changes, 'Classify changed paths')
const classify = classifyStep.run
for (const [name, env, options, expected] of [
  [
    'a pull request rebuilds only the products its files feed',
    {
      GITHUB_EVENT_NAME: 'pull_request',
      PR_NUMBER: '7',
      BASE_REF: 'master',
      HEAD_REF: 'fix/cli',
    },
    { files: ['projects/start-cli/src/main.rs'] },
    { products: 'start-cli', 'start-os': 'false', 'os-image': 'false' },
  ],
  [
    'an integration branch merging into master builds every StartOS image',
    {
      GITHUB_EVENT_NAME: 'pull_request',
      PR_NUMBER: '7',
      BASE_REF: 'master',
      HEAD_REF: 'integration/batch',
      HEAD_REPO: 'Start9Labs/start-technologies',
    },
    { files: ['shared-libs/crates/start-core/src/lib.rs'] },
    { 'start-os': 'true', 'os-image': 'true' },
  ],
  [
    'a fork branch named integration/* gets no image bypass',
    {
      GITHUB_EVENT_NAME: 'pull_request',
      PR_NUMBER: '7',
      BASE_REF: 'master',
      HEAD_REF: 'integration/batch',
      HEAD_REPO: 'someone/start-technologies',
    },
    { files: ['shared-libs/crates/start-core/src/lib.rs'] },
    { 'start-os': 'true', 'os-image': 'false' },
  ],
  [
    'a push rebuilds the products its diff feeds',
    { GITHUB_EVENT_NAME: 'push' },
    { files: ['projects/start-tunnel/src/main.rs'] },
    { products: 'start-tunnel', 'start-os': 'false' },
  ],
  [
    'a new branch rebuilds everything',
    { GITHUB_EVENT_NAME: 'push' },
    { files: ['projects/start-tunnel/src/main.rs'], pages: [] },
    { products: 'start-os start-cli start-registry start-tunnel' },
  ],
  [
    'an unreachable previous commit rebuilds everything',
    { GITHUB_EVENT_NAME: 'push' },
    { files: ['projects/start-tunnel/src/main.rs'], fetches: false },
    { products: 'start-os start-cli start-registry start-tunnel' },
  ],
  [
    'a surviving push rebuilds a product touched only by a superseded push',
    { GITHUB_EVENT_NAME: 'push' },
    {
      files: ['AGENTS.md'],
      diffs: {
        ['d'.repeat(40)]: ['projects/start-cli/src/main.rs', 'AGENTS.md'],
      },
    },
    { products: 'start-cli' },
  ],
  [
    'a page without successful pushes does not hide the preceding build',
    { GITHUB_EVENT_NAME: 'push' },
    {
      files: ['projects/start-cli/src/main.rs'],
      pages: [
        [
          successfulPush({ event: 'pull_request' }),
          successfulPush({ conclusion: 'failure' }),
          successfulPush({ id: 100 }),
          successfulPush({ head_branch: 'integration/other' }),
        ],
        [successfulPush()],
      ],
    },
    { products: 'start-cli' },
  ],
  [
    'moving a file out of a product still rebuilds its former owner',
    { GITHUB_EVENT_NAME: 'pull_request', PR_NUMBER: '7' },
    {
      files: [
        {
          filename: 'archive/cli.rs',
          previous_filename: 'projects/start-cli/src/cli.rs',
        },
      ],
    },
    { products: 'start-cli' },
  ],
  [
    'a dispatch rebuilds everything',
    { GITHUB_EVENT_NAME: 'workflow_dispatch' },
    {},
    {
      products: 'start-os start-cli start-registry start-tunnel',
      'os-image': 'true',
    },
  ],
]) {
  test(`changes: ${name}`, t => {
    const { outputs } = runStep(
      t,
      classify,
      { ...declared(classifyStep), ...env },
      options,
    )
    for (const [key, value] of Object.entries(expected))
      assert.equal(outputs[key], value, key)
  })
}

const wrtClassifyStep = step(
  workflow('start-wrt.yaml').jobs.changes,
  'Classify changed paths',
)
for (const [name, env, options, expected] of [
  [
    'a pull request touching StartWRT builds it',
    { GITHUB_EVENT_NAME: 'pull_request', PR_NUMBER: '7' },
    { files: ['projects/start-wrt/backend/ctrl/src/lib.rs'] },
    'true',
  ],
  [
    'a pull request outside StartWRT skips it',
    { GITHUB_EVENT_NAME: 'pull_request', PR_NUMBER: '7' },
    { files: ['projects/start-cli/src/main.rs'] },
    'false',
  ],
  [
    'a surviving push builds StartWRT changes from a superseded push',
    { GITHUB_EVENT_NAME: 'push' },
    {
      files: ['AGENTS.md'],
      diffs: {
        ['d'.repeat(40)]: ['projects/start-wrt/web/src/main.ts', 'AGENTS.md'],
      },
    },
    'true',
  ],
  [
    'a dispatch builds StartWRT',
    { GITHUB_EVENT_NAME: 'workflow_dispatch' },
    {},
    'true',
  ],
]) {
  test(`start-wrt changes: ${name}`, t => {
    const { outputs, gh } = runStep(
      t,
      wrtClassifyStep.run,
      { ...declared(wrtClassifyStep), ...env },
      options,
    )
    assert.equal(outputs['start-wrt'], expected)
    assert.equal(outputs.products, expected === 'true' ? 'start-wrt' : '')
    for (const [, path] of gh.filter(([, path]) => path.includes('/runs')))
      assert.match(path, /\/actions\/workflows\/start-wrt\.yaml\/runs/)
  })
}

const targetsStep = step(products.jobs.changes, 'Select targets')
const targets = targetsStep.run
const targetEnv = env => ({ ...declared(targetsStep), PLATFORM: 'ALL', ...env })
for (const [name, env, expected] of [
  [
    'a master push publishes StartOS to alpha',
    {
      GITHUB_EVENT_NAME: 'push',
      GITHUB_REF: 'refs/heads/master',
      START_OS: 'true',
    },
    { channel: 'alpha' },
  ],
  [
    'a master push that leaves StartOS alone publishes nothing',
    {
      GITHUB_EVENT_NAME: 'push',
      GITHUB_REF: 'refs/heads/master',
      START_OS: 'false',
    },
    { channel: '' },
  ],
  [
    'an integration push only builds',
    {
      GITHUB_EVENT_NAME: 'push',
      GITHUB_REF: 'refs/heads/integration/batch',
      START_OS: 'true',
    },
    { channel: '' },
  ],
  [
    'a dispatch publishes to the channel it names',
    {
      GITHUB_EVENT_NAME: 'workflow_dispatch',
      DEPLOY: 'beta',
      START_OS: 'true',
    },
    { channel: 'beta' },
  ],
  [
    'a dispatch without a channel only builds',
    {
      GITHUB_EVENT_NAME: 'workflow_dispatch',
      DEPLOY: 'NONE',
      START_OS: 'true',
    },
    { channel: '' },
  ],
  [
    'an empty deploy input only builds',
    { GITHUB_EVENT_NAME: 'workflow_dispatch', DEPLOY: '', START_OS: 'true' },
    { channel: '' },
  ],
  [
    'an unknown deploy input only builds',
    {
      GITHUB_EVENT_NAME: 'workflow_dispatch',
      DEPLOY: 'production',
      START_OS: 'true',
    },
    { channel: '' },
  ],
  [
    'a pull request only builds',
    {
      GITHUB_EVENT_NAME: 'pull_request',
      GITHUB_REF: 'refs/pull/7/merge',
      START_OS: 'true',
    },
    { channel: '' },
  ],
]) {
  test(`targets: ${name}`, t => {
    const { outputs } = runStep(t, targets, targetEnv(env))
    assert.equal(outputs.channel, expected.channel)
  })
}

test('targets: platforms select the arches to build and whether macOS builds', t => {
  const all = runStep(t, targets, targetEnv({ GITHUB_EVENT_NAME: 'push' }))
  assert.deepEqual(JSON.parse(all.outputs.platforms), ALL_PLATFORMS)
  assert.deepEqual(JSON.parse(all.outputs.arches), [
    'aarch64',
    'riscv64',
    'x86_64',
  ])
  assert.equal(all.outputs.macos, 'true')
  for (const [platform, arch] of [
    ['raspberrypi', 'aarch64'],
    ['x86_64-nvidia', 'x86_64'],
    ['riscv64-nonfree', 'riscv64'],
  ]) {
    const one = runStep(
      t,
      targets,
      targetEnv({ PLATFORM: platform, GITHUB_EVENT_NAME: 'workflow_dispatch' }),
    )
    assert.deepEqual(JSON.parse(one.outputs.platforms), [platform])
    assert.deepEqual(JSON.parse(one.outputs.arches), [arch])
    assert.equal(one.outputs.macos, 'false')
  }
})

test('every product builds from one compile per target, on standard runners', () => {
  const build = products.jobs.build
  const steps = build.steps
  const compile = steps.findIndex(
    step =>
      step.run ===
      'make rust-products RUST_PRODUCTS="${{ needs.changes.outputs.products }}"',
  )
  assert.ok(compile !== -1)
  for (const [product, recipe] of [
    ['start-os', 'make compiled-${{ matrix.arch }}.tar.gz'],
    ['start-cli', 'make start-cli-deb'],
    ['start-registry', 'make start-registry-deb'],
    ['start-tunnel', 'make start-tunnel-deb'],
  ]) {
    const packaging = steps.find(step => step.run === recipe)
    assert.ok(packaging, recipe)
    assert.equal(packaging.if, `needs.changes.outputs.${product} == 'true'`)
    assert.ok(compile < steps.indexOf(packaging))
  }
  for (const file of [
    'build-products.yaml',
    'startos-iso.yaml',
    'startos-image.yaml',
  ])
    assert.doesNotMatch(
      readFileSync(join(REPO, '.github/workflows', file), 'utf8'),
      /\b(?:amd64|aarch64)-fast\b|32-cores/,
      file,
    )
})

test('the CLI binaries cover every triple a release attaches', () => {
  const triples = readFileSync(join(REPO, 'scripts/manage-release.sh'), 'utf8')
    .match(/^CLI_TRIPLES="([^"]+)"$/m)[1]
    .split(' ')
  const linux = products.jobs.build.steps.find(
    step =>
      step.uses === 'actions/upload-artifact@v7' &&
      step.with.name?.endsWith('-unknown-linux-musl'),
  )
  const built = [
    ...['x86_64', 'aarch64', 'riscv64'].map(arch =>
      evaluate(
        `'start-cli_' + (${linux.with.name.match(/\$\{\{(.*)\}\}/)[1]}) + '-unknown-linux-musl'`,
        { matrix: { arch } },
      ),
    ),
    ...products.jobs['cli-macos'].strategy.matrix.triple.map(
      triple => `start-cli_${triple}`,
    ),
  ]
  assert.deepEqual(
    built.sort(),
    triples.map(triple => `start-cli_${triple}`).sort(),
  )
})

test('registry image platforms match the downloaded deb architectures', t => {
  const mapPlatforms = step(
    products.jobs['registry-image'],
    'Map matrix.arch to docker platform',
  ).run
  for (const [arches, expected] of [
    [['x86_64'], ['linux/amd64']],
    [['aarch64'], ['linux/arm64']],
    [['riscv64'], ['linux/riscv64']],
    [
      ['x86_64', 'aarch64', 'riscv64'],
      ['linux/amd64', 'linux/arm64', 'linux/riscv64'],
    ],
  ]) {
    const f = fixture(t)
    for (const arch of arches)
      f.write(`start-registry-0.4.0-aaaaaaa_${arch}.deb`, '')
    const output = f.write('env', '')
    const result = f.run('bash', ['-e', '-c', mapPlatforms], {
      env: { ...f.env, GITHUB_ENV: output },
      encoding: 'utf8',
    })
    assert.equal(result.status, 0, result.stdout + result.stderr)
    const platforms = readFileSync(output, 'utf8')
      .trim()
      .replace(/^DOCKER_PLATFORM=/, '')
      .split(',')
    assert.deepEqual(platforms.sort(), expected.sort())
  }
})

test('build environments: release-equivalent on master and NONE, dev elsewhere', () => {
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
      evaluate(products.env.ENVIRONMENT, context(event, input, ref)),
      expected,
    )
})

test('a publishing build dispatches startos-iso.yaml pinned to its commit', t => {
  const publish = products.jobs.publish
  assert.deepEqual(publish.needs, ['changes', 'build', 'rootfs'])
  for (const [channel, rootfs, expected] of [
    ['alpha', 'success', true],
    ['', 'success', false],
    ['alpha', 'failure', false],
    ['alpha', 'skipped', false],
  ])
    assert.equal(
      evaluate(publish.if, {
        needs: {
          changes: { outputs: { channel } },
          rootfs: { result: rootfs },
        },
      }),
      expected,
    )
  const platforms = JSON.stringify(ALL_PLATFORMS)
  const { gh } = runStep(t, publish.steps[0].run, {
    GITHUB_REF_NAME: 'master',
    GITHUB_RUN_ID: '42',
    CHANNEL: 'alpha',
    PLATFORMS: platforms,
    ENVIRONMENT: '',
    FORCE_OVERWRITE: 'false',
  })
  assert.equal(gh.length, 1)
  const [args] = gh
  assert.deepEqual(args.slice(0, 7), [
    'workflow',
    'run',
    'startos-iso.yaml',
    '-R',
    'Start9Labs/start-technologies',
    '--ref',
    'master',
  ])
  const fields = Object.fromEntries(
    args
      .filter((arg, i) => args[i - 1] === '-f')
      .map(field => field.split(/=(.*)/s).slice(0, 2)),
  )
  assert.deepEqual(fields, {
    'build-run-id': '42',
    commit: SHA,
    channel: 'alpha',
    platforms,
    environment: '',
    'force-overwrite': 'false',
  })
  assert.deepEqual(
    Object.keys(iso.on.workflow_dispatch.inputs).sort(),
    Object.keys(fields).sort(),
  )
})

const checkBuild = step(iso.jobs['deploy-prepare'], 'Check the build run').run
for (const [name, run, accepted] of [
  ['a master push build of the commit publishes', {}, true],
  [
    'a dispatched build of the commit publishes',
    { event: 'workflow_dispatch' },
    true,
  ],
  ['a pull request build never publishes', { event: 'pull_request' }, false],
  ['a build of another commit is refused', { head_sha: 'e'.repeat(40) }, false],
  [
    'a fork build is refused',
    { head_repository: { full_name: 'someone/start-technologies' } },
    false,
  ],
  [
    "another workflow's run is refused",
    { path: '.github/workflows/start-wrt.yaml' },
    false,
  ],
]) {
  test(`publish: ${name}`, t => {
    const f = fixture(t)
    const response = {
      head_sha: SHA,
      head_repository: { full_name: 'Start9Labs/start-technologies' },
      path: '.github/workflows/build-products.yaml',
      event: 'push',
      ...run,
    }
    f.write('run.json', JSON.stringify(response))
    f.nodeExecutable(
      'gh',
      `import { readFileSync } from 'node:fs'
process.stdout.write(readFileSync(${JSON.stringify(join(f.root, 'run.json'))}))
`,
    )
    const result = f.run('bash', ['-e', '-c', checkBuild], {
      env: {
        ...f.env,
        PATH: `${f.root}:${f.env.PATH}`,
        GITHUB_REPOSITORY: 'Start9Labs/start-technologies',
        BUILD_RUN_ID: '42',
        COMMIT: SHA,
      },
      encoding: 'utf8',
    })
    assert.equal(result.status === 0, accepted, result.stdout + result.stderr)
  })
}

test('publishing runs serialize per channel; builds per event and ref', () => {
  for (const channel of ['alpha', 'beta'])
    assert.equal(
      interpolate(iso.concurrency.group, { inputs: { channel } }),
      `startos-iso-publish-${channel}`,
    )
  assert.equal(iso.concurrency['cancel-in-progress'], false)
  for (const [event, ref, cancels] of [
    ['push', 'refs/heads/master', false],
    ['workflow_dispatch', 'refs/heads/release/example', false],
    ['pull_request', 'refs/pull/1/merge', true],
  ]) {
    const ctx = context(event, {}, ref)
    assert.equal(
      interpolate(products.concurrency.group, ctx),
      `build-${event}-${ref}`,
    )
    assert.equal(
      evaluate(products.concurrency['cancel-in-progress'], ctx),
      cancels,
    )
  }
  assert.deepEqual(workflow('startos-image.yaml').jobs.deploy.concurrency, {
    group: 'startos-deploy-${{ inputs.platform }}',
    'cancel-in-progress': false,
  })
})

// Each product's first deploy job, and for a gated one the job whose success it waits for.
for (const [file, product, job, gate] of [
  ['startos-iso.yaml', 'start-os', 'deploy-prepare'],
  ['start-wrt.yaml', 'start-wrt', 'deploy', 'image'],
]) {
  const source = workflow(file)
  const deploy = source.jobs[job]
  const gated = (ctx, result = 'success') => ({
    ...ctx,
    needs: { [gate]: { result } },
  })
  if (gate)
    test(`${product} deploys only for eligible channels after a successful ${gate}`, () => {
      assert.equal(source.jobs['publisher-cli'], undefined)
      assert.deepEqual(deploy.needs, [gate])
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
          context('workflow_dispatch', {
            deploy: 'alpha',
            platform: 'aarch64',
          }),
          true,
        ])
      for (const [ctx, expected] of cases) {
        assert.equal(
          evaluate(deploy.if, gated(ctx)),
          expected,
          JSON.stringify(ctx),
        )
        for (const result of ['failure', 'cancelled', 'skipped'])
          assert.equal(evaluate(deploy.if, gated(ctx, result)), false)
        assert.equal(
          evaluate(deploy.if, { ...gated(ctx), cancelled: () => true }),
          false,
        )
      }
    })

  test(`${product} installs the released CLI and capability-checks before its registry writes`, () => {
    const steps = deploy.steps
    const install = steps.find(step => step.name === 'Install start-cli')
    assert.equal(
      install.uses,
      'Start9Labs/start-technologies/.github/actions/install-start-cli@master',
    )
    assert.equal(deploy['runs-on'], 'ubuntu-latest')
    const check = steps[steps.indexOf(install) + 1]
    assert.equal(
      check.run.trim(),
      `./scripts/manage-release.sh check-registry-notes-cli ${product}`,
    )
    const compiler = steps.findIndex(
      step => step.name === 'Compile release notes',
    )
    assert.ok(compiler < steps.indexOf(install))
    const writes = steps.filter(step =>
      /s3cmd (put|del)|s3-put-once\.sh|registry os (version (add|remove)|asset add)/.test(
        step.run || '',
      ),
    )
    assert.ok(writes.length >= (product === 'start-os' ? 1 : 3))
    for (const step of writes)
      assert.ok(steps.indexOf(check) < steps.indexOf(step))
  })
}

test('start-os images deploy per platform only once deploy-prepare registered the version', () => {
  const iso = workflow('startos-iso.yaml')
  const leg = workflow('startos-image.yaml').jobs.deploy
  assert.ok(iso.jobs.image.needs.includes('deploy-prepare'))
  assert.equal(iso.jobs.image.uses, './.github/workflows/startos-image.yaml')
  assert.deepEqual(leg.needs, ['build'])
  for (const input of ['channel', 'registry', 'version', 's3-bucket', 's3-cdn'])
    assert.equal(
      iso.jobs.image.with[input],
      `\${{ needs.deploy-prepare.outputs.${input} }}`,
    )
  const prepare = iso.jobs['deploy-prepare']
  const registered = prepare.steps.find(step => step.id === 'registered')
  const register = prepare.steps.find(
    step => step.name === 'Register OS version',
  )
  assert.ok(prepare.steps.indexOf(register) < prepare.steps.indexOf(registered))
  assert.equal(
    prepare.outputs.channel,
    '${{ steps.registered.outputs.channel }}',
  )
  for (const [channel, expected] of [
    ['', false],
    ['alpha', true],
  ])
    assert.equal(evaluate(leg.if, { inputs: { channel } }), expected)
  const install = leg.steps.findIndex(step => step.name === 'Install start-cli')
  const writes = leg.steps.filter(step =>
    /s3-put-once\.sh|registry os asset (add|remove)/.test(step.run || ''),
  )
  assert.ok(writes.length >= 2)
  for (const step of writes) assert.ok(install < leg.steps.indexOf(step))
})

test('StartOS upload tooling follows the workflow commit when publishing an older build', () => {
  const source = workflow('startos-image.yaml')
  const ctx = { inputs: { commit: 'b'.repeat(40) }, github: { sha: SHA } }
  const checkout = job =>
    source.jobs[job].steps.find(step => step.uses === 'actions/checkout@v6')
  assert.equal(evaluate(checkout('build').with.ref, ctx), ctx.inputs.commit)
  const deploy = checkout('deploy')
  assert.equal(evaluate(deploy.with.ref, ctx), SHA)
  assert.equal(deploy.with['sparse-checkout'], 'scripts/s3-put-once.sh')
})

test('product builds trigger on every push and pull request and classify in the changes job', () => {
  const wrt = workflow('start-wrt.yaml')
  for (const event of ['push', 'pull_request']) {
    assert.equal(products.on[event].paths, undefined)
    assert.equal(wrt.on[event].paths, undefined)
  }
  for (const job of ['compile', 'image']) {
    assert.equal(wrt.jobs[job].needs, 'changes', job)
    assert.match(
      wrt.jobs[job].if,
      /needs\.changes\.outputs\.start-wrt == 'true'/,
      job,
    )
  }
  assert.deepEqual(wrt.jobs.deploy.needs, ['image'])
})

test('the rootfs builds once per architecture for every image this run or its publish builds', () => {
  const { image, rootfs } = products.jobs
  assert.deepEqual(image.needs, ['changes', 'build', 'rootfs'])
  for (const [event, osImage, channel, rootfsRuns, imageRuns] of [
    ['pull_request', 'true', '', true, true],
    ['pull_request', 'false', '', false, false],
    ['push', 'false', '', true, true],
    ['push', 'false', 'alpha', true, false],
  ]) {
    const outputs = { 'start-os': 'true', 'os-image': osImage, channel }
    const context = {
      github: { event_name: event },
      needs: {
        changes: { outputs },
        build: { result: 'success' },
        rootfs: { result: 'success' },
      },
    }
    assert.equal(evaluate(rootfs.if, context), rootfsRuns, `${event} rootfs`)
    assert.equal(evaluate(image.if, context), imageRuns, `${event} image`)
  }
  assert.equal(
    evaluate(image.if, {
      github: { event_name: 'push' },
      needs: {
        changes: { outputs: { 'start-os': 'true', channel: '' } },
        rootfs: { result: 'failure' },
      },
    }),
    false,
  )
})

test('compiled-artifact consumers grant Actions read permission', () => {
  for (const permissions of [
    products.jobs.rootfs.permissions,
    products.jobs.image.permissions,
    iso.permissions,
  ])
    assert.equal(permissions.actions, 'read')
})
