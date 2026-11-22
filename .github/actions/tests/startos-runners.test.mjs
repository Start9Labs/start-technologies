import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { chmodSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { fixture } from '../../../scripts/tests/fixtures.mjs'

const load = name =>
  yaml.load(
    readFileSync(new URL(`../../workflows/${name}`, import.meta.url), 'utf8'),
  )
const workflow = load('startos-iso.yaml')
const products = load('build-products.yaml')
const image = load('startos-image.yaml').jobs.build
const runners = yaml.load(image['runs-on'].match(/fromJson\('([\s\S]*?)'\)/)[1])
const architectures = yaml.load(
  image.env.ARCH.match(/fromJson\('([\s\S]*?)'\)/)[1],
)
const bootstrap = yaml.load(
  readFileSync(
    new URL('../riscv-action-runtime/action.yml', import.meta.url),
    'utf8',
  ),
).runs.steps[0]
const rootfs = products.jobs.rootfs
const rootfsRunners = yaml.load(
  rootfs['runs-on'].match(/fromJson\('([\s\S]*?)'\)/)[1],
)

for (const platform of ['riscv64', 'riscv64-nonfree']) {
  test(`${platform} image assembly stays native`, () => {
    assert.equal(runners[platform], 'ubuntu-24.04-riscv')
    assert.equal(architectures[platform], 'riscv64')
  })
}

test('each architecture builds its container runtime rootfs natively', () => {
  assert.deepEqual(rootfsRunners, {
    x86_64: 'ubuntu-latest',
    aarch64: 'ubuntu-24.04-arm',
    riscv64: 'ubuntu-24.04-riscv',
  })
  for (const [platform, arch] of Object.entries(architectures))
    assert.equal(rootfsRunners[arch], runners[platform], platform)
})

test('the native runtime bootstrap precedes every JavaScript action after checkout', () => {
  for (const [job, arch] of [
    [image, 'env.ARCH'],
    [rootfs, 'matrix.arch'],
  ]) {
    const actions = job.steps.filter(step => step.uses)
    assert.equal(actions[0].uses, 'actions/checkout@v6')
    assert.equal(actions[1].uses, './.github/actions/riscv-action-runtime')
    assert.equal(actions[1].if, `\${{ ${arch} == 'riscv64' }}`)
    assert.equal(actions[1].with['nodejs-version'], '${{ env.NODEJS_VERSION }}')
  }
  assert.equal(bootstrap.env.NODEJS_VERSION, '${{ inputs.nodejs-version }}')
  for (const caller of [workflow, products])
    assert.equal(
      caller.jobs.image.with['nodejs-version'],
      caller.env.NODEJS_VERSION,
    )
})

for (const [name, validChecksum, startsJavaScript] of [
  ['replaces the action host and exposes shell Node', true, true],
  ['preserves the action host after checksum failure', false, true],
  ['preserves the action host after JavaScript startup failure', true, false],
]) {
  test(`runtime bootstrap ${name}`, t => {
    const f = fixture(t)
    const version = workflow.env.NODEJS_VERSION
    const nodeDir = `node-v${version}-linux-riscv64`
    const nodeSource = `#!/bin/bash
for arg; do
  if [ "$arg" = --version ]; then
    printf 'v${version}\\n'
    exit
  fi
done
${startsJavaScript ? 'exec "$PROBE_NODE" "$@"' : 'exit 132'}
`
    chmodSync(f.write(`archive/${nodeDir}/bin/node`, nodeSource), 0o755)
    const tarball = `${nodeDir}.tar.xz`
    const packed = f.run('tar', ['-cJf', tarball, '-C', 'archive', nodeDir])
    assert.equal(packed.status, 0, String(packed.stderr))
    const checksum = validChecksum
      ? createHash('sha256')
          .update(readFileSync(join(f.root, tarball)))
          .digest('hex')
      : '0'.repeat(64)
    f.write('SHASUMS256.txt', `${checksum}  ${tarball}\n`)
    const oldNode = '#!/bin/sh\nprintf "v24.7.0\\n"\n'
    const runnerNode = f.write(
      'runner cache/externals/node24/bin/node',
      oldNode,
    )
    chmodSync(runnerNode, 0o755)
    const worker = f.write('runner cache/bin/Runner.Worker', '')
    f.nodeExecutable('readlink', `console.log(${JSON.stringify(worker)})`)
    f.nodeExecutable(
      'curl',
      `import { copyFileSync, readFileSync } from 'node:fs'
import { basename, join } from 'node:path'
const name = basename(process.argv.at(-1))
const source = join(${JSON.stringify(f.root)}, name)
if (process.argv.includes('-fsSLO')) copyFileSync(source, name)
else process.stdout.write(readFileSync(source))`,
    )
    const temp = join(f.root, 'runner temp')
    f.write('runner temp/.keep', '')
    const pathFile = f.write('path', '/existing/bin\n')
    const result = f.run('bash', ['-c', bootstrap.run], {
      env: {
        ...f.env,
        NODEJS_VERSION: version,
        PROBE_NODE: process.execPath,
        PATH: `${f.root}:${f.env.PATH}`,
        RUNNER_TEMP: temp,
        GITHUB_PATH: pathFile,
      },
      encoding: 'utf8',
    })
    const healthy = validChecksum && startsJavaScript
    assert.equal(result.status === 0, healthy, result.stdout + result.stderr)
    if (healthy) {
      assert.equal(readFileSync(`${runnerNode}.real`, 'utf8'), nodeSource)
      const host = f.run(
        runnerNode,
        ['-p', 'process.execArgv[0] + " " + process.env["INPUT_CACHE-IMAGE"]'],
        {
          env: {
            ...f.env,
            PROBE_NODE: process.execPath,
            'INPUT_CACHE-IMAGE': 'true',
          },
          encoding: 'utf8',
        },
      )
      assert.equal(host.stdout.trim(), '--single-threaded true', host.stderr)
    } else {
      assert.equal(readFileSync(runnerNode, 'utf8'), oldNode)
    }
    assert.equal(
      readFileSync(pathFile, 'utf8'),
      `/existing/bin\n${healthy ? `${temp}/${nodeDir}/bin\n` : ''}`,
    )
    if (healthy) {
      assert.match(result.stdout, /Bundled action runtime: v24\.7\.0/)
      assert.ok(result.stdout.includes(`Updated action runtime: v${version}`))
    }
  })
}
