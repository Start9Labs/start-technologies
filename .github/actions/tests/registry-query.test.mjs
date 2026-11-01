import assert from 'node:assert/strict'
import {
  mkdtempSync,
  readFileSync,
  writeFileSync,
  rmSync,
  existsSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createRequire } from 'node:module'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'

const require = createRequire(import.meta.url)
const yaml = require('js-yaml')

function runQueryStep(
  t,
  workflow,
  job,
  name,
  { status = 0, output = '' } = {},
) {
  const source = yaml.load(
    readFileSync(
      new URL(`../../workflows/${workflow}.yml`, import.meta.url),
      'utf8',
    ),
  )
  const step = source.jobs[job].steps.find(step => step.name === name)
  const values = {
    'steps.extract.outputs.package_id': 'gitea-probe',
    'steps.extract.outputs.version': '1.0.0:0',
    'steps.version.outputs.package_id': 'gitea-probe',
    'steps.version.outputs.version': '1.0.0:0',
    'inputs.REFERENCE_REGISTRY': 'https://registry.invalid',
    'inputs.RELEASE_REGISTRY': 'https://registry.invalid',
  }
  const script = step.run.replace(
    /\$\{\{\s*([\w.]+)\s*\}\}/g,
    (_, reference) => {
      assert.ok(reference in values, reference)
      return values[reference]
    },
  )
  const cwd = mkdtempSync(join(tmpdir(), 'registry-query-'))
  t.after(() => rmSync(cwd, { recursive: true, force: true }))
  writeFileSync(
    join(cwd, 'start-cli'),
    `#!/bin/bash
printf '%s\\n' "$*" >> "$CALLS"
case "$*" in
  *'registry package get'*) printf '%s' "$QUERY_OUTPUT"; exit "$QUERY_STATUS" ;;
  *'registry package remove'*) exit 0 ;;
  *) exit 99 ;;
esac
`,
    { mode: 0o755 },
  )
  const result = spawnSync('bash', ['-e', '-o', 'pipefail', '-c', script], {
    cwd,
    encoding: 'utf8',
    env: {
      ...process.env,
      PATH: `${cwd}:${process.env.PATH}`,
      QUERY_STATUS: String(status),
      QUERY_OUTPUT: output,
      CALLS: join(cwd, 'calls'),
      GITHUB_OUTPUT: join(cwd, 'outputs'),
    },
  })
  return {
    ...result,
    calls: readFileSync(join(cwd, 'calls'), 'utf8'),
    outputs: existsSync(join(cwd, 'outputs'))
      ? readFileSync(join(cwd, 'outputs'), 'utf8')
      : '',
  }
}

test('tagging reports a failed reference query under bash -e', t => {
  const result = runQueryStep(
    t,
    'tagAndRelease',
    'Tag',
    'Check production registry',
    { status: 42, output: 'query unavailable' },
  )
  assert.equal(result.status, 1)
  assert.match(
    result.stdout,
    /::error::Failed to query production registry: query unavailable/,
  )
  assert.equal(result.outputs, '')
})

for (const [output, skip] of [
  ['', 'false'],
  ['{"version":"1.0.0:0"}', 'true'],
]) {
  test(`reference query sets skip=${skip} for ${output ? 'published' : 'absent'} versions`, t => {
    const result = runQueryStep(
      t,
      'tagAndRelease',
      'Tag',
      'Check production registry',
      { output },
    )
    assert.equal(result.status, 0)
    assert.equal(result.outputs, `skip=${skip}\n`)
  })
}

test('release query failure warns without aborting under bash -e', t => {
  const result = runQueryStep(
    t,
    'release',
    'Publish',
    'Remove existing version from registry',
    { status: 42, output: 'query unavailable' },
  )
  assert.equal(result.status, 0)
  assert.match(
    result.stdout,
    /::warning::Could not query registry: query unavailable/,
  )
  assert.doesNotMatch(result.calls, /package remove/)
})

for (const [output, remove] of [
  ['', false],
  ['{"version":"1.0.0:0"}', true],
]) {
  test(`release query ${remove ? 'removes the published version' : 'leaves an absent version alone'}`, t => {
    const result = runQueryStep(
      t,
      'release',
      'Publish',
      'Remove existing version from registry',
      { output },
    )
    assert.equal(result.status, 0)
    assert.equal(
      result.calls.includes('registry package remove gitea-probe 1.0.0:0'),
      remove,
    )
  })
}
