import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const yaml = readFileSync(
  new URL('../../../.github/workflows/test.yaml', import.meta.url),
  'utf8',
)
const job = yaml.match(
  /^  complexity:\n[\s\S]*?(?=^  [\w-]+:|$(?![\s\S]))/m,
)?.[0]

test('Complexity verifies non-draft code PR heads, not push or merged trees', () => {
  assert.ok(job)
  const condition = job
    .match(/^    if: >-\n((?:      .*\n)+)/m)?.[1]
    .trim()
    .replace(/\s*\n\s*/g, ' ')
  assert.ok(condition)
  const evaluate = new Function(
    'github',
    'needs',
    'cancelled',
    `return (${condition})`,
  )
  for (const [name, event, draft, code, prettier, cancelled, expected] of [
    ['code PR', 'pull_request', false, 'true', 'success', false, true],
    ['master push', 'push', null, 'true', 'success', false, false],
    ['workflow call', 'workflow_call', null, 'true', 'success', false, false],
    ['draft PR', 'pull_request', true, 'true', 'success', false, false],
    ['docs PR', 'pull_request', false, 'false', 'success', false, false],
    [
      'formatting failed',
      'pull_request',
      false,
      'true',
      'failure',
      false,
      false,
    ],
    [
      'formatting skipped',
      'pull_request',
      false,
      'true',
      'skipped',
      false,
      false,
    ],
    ['cancelled', 'pull_request', false, 'true', 'success', true, false],
  ]) {
    assert.equal(
      evaluate(
        { event_name: event, event: { pull_request: { draft } } },
        { prettier: { result: prettier }, changes: { outputs: { code } } },
        () => cancelled,
      ),
      expected,
      name,
    )
  }
  assert.match(job, /ref: \$\{\{ github\.event\.pull_request\.head\.sha \}\}/)
  assert.doesNotMatch(job, /github\.sha|setup-python/)
  assert.match(job, /node-version: '24'/)
  assert.match(job, /run: make complexity-test/)
})
