import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO, fixture } from '../../../scripts/tests/fixtures.mjs'

const action = yaml.load(
  readFileSync(join(REPO, '.github/actions/setup-build/action.yml'), 'utf8'),
)
const workflow = yaml.load(
  readFileSync(join(REPO, '.github/workflows/test.yaml'), 'utf8'),
)
const cleanup = action.runs.steps.find(step => step.name === 'Free disk space')

test('heavy test jobs retain shared disk cleanup before building', () => {
  for (const name of [
    'start-core',
    'start-wrt',
    'rust-crates',
    'scripts',
    'format',
    'generated',
  ]) {
    const steps = workflow.jobs[name].steps
    const setup = steps.find(
      step => step.uses === './.github/actions/setup-build',
    )
    assert.ok(setup, name)
    assert.equal(
      setup.with['free-space'] ?? action.inputs['free-space'].default,
      'true',
      name,
    )
    assert.ok(
      steps.indexOf(setup) <
        steps.findIndex(step => step.run?.includes('make ')),
      name,
    )
  }
  assert.equal(cleanup.if, "inputs.free-space == 'true'")
})

for (const [name, available, cleans] of [
  ['frees space and reports capacity before and after', '12G', true],
  ['skips a runner that already has enough space', '88G', false],
]) {
  test(`shared cleanup ${name}`, t => {
    const { root, env, nodeExecutable, run } = fixture(t)
    for (const command of ['df', 'sudo']) {
      nodeExecutable(
        command,
        `import { appendFileSync } from 'node:fs'
appendFileSync(process.env.COMMAND_LOG, JSON.stringify([${JSON.stringify(command)}, ...process.argv.slice(2)]) + '\\n')
if (process.argv.includes('--output=avail')) console.log('Avail\\n' + process.env.AVAILABLE)
`,
      )
    }
    const log = join(root, 'commands.jsonl')
    const result = run('bash', ['-e', '-c', cleanup.run], {
      encoding: 'utf8',
      env: {
        ...env,
        PATH: `${root}:${env.PATH}`,
        COMMAND_LOG: log,
        AVAILABLE: available,
        GITHUB_ACTION_PATH: join(REPO, '.github/actions/setup-build'),
        AGENT_TOOLSDIRECTORY: join(root, 'toolcache'),
      },
    })
    assert.equal(result.status, 0, result.stderr)
    const calls = readFileSync(log, 'utf8')
      .trim()
      .split('\n')
      .map(line => JSON.parse(line))
    assert.deepEqual(calls.slice(0, 2), [
      ['df', '--output=avail', '-BG', '/'],
      ['df', '-h', '/'],
    ])
    if (!cleans) {
      assert.equal(calls.length, 2)
      return
    }
    assert.deepEqual(calls.at(-1), ['df', '-h', '/'])
    assert.ok(calls.length > 3)
    assert.ok(calls.slice(2, -1).every(([command]) => command === 'sudo'))
    assert.ok(
      calls.some(
        call => call.join(' ') === `sudo rm -rf ${join(root, 'toolcache')}`,
      ),
    )
  })
}
