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
  for (const name of ['test', 'format', 'generated']) {
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

test('shared cleanup reports root filesystem capacity before and after freeing space', t => {
  const { root, env, nodeExecutable, run } = fixture(t)
  for (const command of ['df', 'sudo']) {
    nodeExecutable(
      command,
      `import { appendFileSync } from 'node:fs'
appendFileSync(process.env.COMMAND_LOG, JSON.stringify([${JSON.stringify(command)}, ...process.argv.slice(2)]) + '\\n')
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
      AGENT_TOOLSDIRECTORY: join(root, 'toolcache'),
    },
  })
  assert.equal(result.status, 0, result.stderr)
  const calls = readFileSync(log, 'utf8')
    .trim()
    .split('\n')
    .map(line => JSON.parse(line))
  assert.deepEqual(calls[0], ['df', '-h', '/'])
  assert.deepEqual(calls.at(-1), ['df', '-h', '/'])
  assert.ok(calls.length > 2)
  assert.ok(calls.slice(1, -1).every(([command]) => command === 'sudo'))
})
