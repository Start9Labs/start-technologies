import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import yaml from 'js-yaml'
import { REPO } from '../../../scripts/tests/fixtures.mjs'

const setup = './.github/actions/setup-release-tests'
const workflow = yaml.load(
  readFileSync(join(REPO, '.github/workflows/test.yaml'), 'utf8'),
)

for (const command of ['make test', 'make release-notes-test']) {
  test(`${command} installs release-tooling dependencies before running`, () => {
    const callers = Object.values(workflow.jobs).filter(job =>
      job.steps?.some(step => step.run === command),
    )
    assert.ok(callers.length > 0)
    for (const { steps } of callers) {
      const install = steps.findIndex(step => step.uses === setup)
      assert.ok(
        install >
          steps.findIndex(step => step.uses?.startsWith('actions/checkout@')),
      )
      assert.ok(install < steps.findIndex(step => step.run === command))
    }
  })
}

test('release-tooling setup installs the real archive and signing tools', () => {
  const action = yaml.load(
    readFileSync(join(REPO, setup, 'action.yml'), 'utf8'),
  )
  const script = action.runs.steps.map(step => step.run ?? '').join('\n')
  assert.match(script, /apt-get update/)
  for (const pkg of ['apt-utils', 'dpkg-dev', 'gnupg']) {
    assert.match(script, new RegExp(`apt-get install[^\n]*\\b${pkg}\\b`))
  }
})
