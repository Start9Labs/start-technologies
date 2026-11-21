import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'

const require = createRequire(import.meta.url)
const yaml = require('js-yaml')
const { generate } = require('../../../scripts/generate-forgejo-workflows.cjs')
const root = fileURLToPath(new URL('../../../', import.meta.url))
const generated = await generate(root)
const developerKey = yaml.load(
  readFileSync(
    join(root, '.github/actions/configure-developer-key/action.yml'),
    'utf8',
  ),
)

function enabled(step, inputs, privateRepository) {
  return Boolean(
    new Function('inputs', 'github', `return (${step.if.slice(3, -2)})`)(
      inputs,
      { event: { repository: { private: privateRepository } } },
    ),
  )
}

for (const forge of ['github', 'forgejo']) {
  const document = name => {
    const file = `.${forge}/workflows/${name}.yml`
    return yaml.load(
      forge === 'github'
        ? readFileSync(join(root, file), 'utf8')
        : generated.get(file),
    )
  }

  test(`${forge} private versions remain protected by the permanent-version gate`, t => {
    const home = mkdtempSync(join(tmpdir(), 'private-package-gate-'))
    t.after(() => rmSync(home, { recursive: true, force: true }))
    const output = join(home, 'output')
    const env = {
      ...process.env,
      HOME: home,
      PATH: `${home}:${process.env.PATH}`,
      DEV_KEY: 'fixture-developer-key',
      GITHUB_OUTPUT: output,
    }
    writeFileSync(
      join(home, 'start-cli'),
      `#!/bin/bash
if [ -f "$HOME/.startos/developer.key.pem" ] && [ "$(< "$HOME/.startos/developer.key.pem")" = "$DEV_KEY" ]; then
  printf '%s\\n' 'Published private package'
fi
`,
      { mode: 0o755 },
    )
    const steps = document('tagAndRelease').jobs.Tag.steps
    const check = steps.findIndex(step => step.id === 'check')
    for (const step of steps.slice(0, check)) {
      if (step.uses?.includes('/configure-developer-key')) {
        assert.equal(step.with.key, '${{ secrets.DEV_KEY }}')
        for (const configure of developerKey.runs.steps) {
          execFileSync('bash', ['-e', '-o', 'pipefail', '-c', configure.run], {
            env,
          })
        }
      }
    }
    const values = {
      'steps.extract.outputs.package_id': 'private-fixture',
      'steps.extract.outputs.version': '1.0.0:0',
      'inputs.REFERENCE_REGISTRY': 'https://registry.example',
    }
    const script = steps[check].run.replace(
      /\$\{\{\s*(.*?)\s*\}\}/g,
      (_, key) => {
        assert.ok(Object.hasOwn(values, key), key)
        return values[key]
      },
    )
    execFileSync('bash', ['-e', '-o', 'pipefail', '-c', script], { env })
    assert.match(readFileSync(output, 'utf8'), /^skip=true$/m)
  })

  test(`${forge} private repository publication rejects credential-only release URLs`, () => {
    const release = document('release')
    const validation = release.jobs.DiscoverMatrix.steps.find(
      step => step.name === 'Validate private repository hosting',
    )
    const inputs = {
      RELEASE_REGISTRY: 'https://registry.example',
      S3_S9PKS_BASE_URL: '',
    }
    assert.equal(enabled(validation, inputs, true), true)
    assert.equal(spawnSync('bash', ['-e', '-c', validation.run]).status, 1)
    assert.equal(enabled(validation, inputs, false), false)
    assert.equal(
      enabled(validation, { ...inputs, RELEASE_REGISTRY: '' }, true),
      false,
    )
  })

  for (const [privateRepository, s3, expected] of [
    [true, 'https://bucket.example', ['Publish to Registry via S3']],
    [false, '', ['Publish to Registry via GitHub']],
    [
      false,
      'https://bucket.example',
      ['Publish to Registry via S3', 'Add GitHub release mirrors'],
    ],
  ]) {
    test(`${forge} download routes for private=${privateRepository}, s3=${Boolean(s3)}`, () => {
      const steps = document('release').jobs.Publish.steps.filter(
        step =>
          step.name.startsWith('Publish to Registry') ||
          step.name === 'Add GitHub release mirrors',
      )
      const inputs = {
        RELEASE_REGISTRY: 'https://registry.example',
        S3_S9PKS_BASE_URL: s3,
      }
      assert.deepEqual(
        steps
          .filter(step => enabled(step, inputs, privateRepository))
          .map(step => step.name),
        expected,
      )
    })
  }
}
