import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO } from '../../../scripts/tests/fixtures.mjs'

const workflow = yaml.load(
  readFileSync(join(REPO, '.github/workflows/test.yaml'), 'utf8'),
)
const targetsRunByCi = new Set(
  Object.values(workflow.jobs)
    .flatMap(job => job.steps ?? [])
    .flatMap(step => [
      ...(step.run ?? '').matchAll(/\bmake((?:[ \t]+[\w.-]+)+)/g),
    ])
    .flatMap(([, args]) => args.trim().split(/\s+/))
    .filter(arg => !arg.startsWith('-')),
)

function prerequisites(file, target) {
  const rule = readFileSync(join(REPO, file), 'utf8').match(
    new RegExp(`^${target}:([^\\n]*)$`, 'm'),
  )
  assert.ok(rule, `${file} defines ${target}`)
  return rule[1].split(/\s+/).filter(name => name && name !== '|')
}

for (const [file, target] of [
  ['Makefile', 'test'],
  ['shared-libs/crates/start-core/build.mk', 'generated-artifacts-check'],
]) {
  test(`CI runs every target make ${target} aggregates`, () => {
    const targets = prerequisites(file, target)
    assert.ok(targets.length > 1)
    assert.deepEqual(
      targets.filter(name => !targetsRunByCi.has(name)),
      [],
    )
  })
}
