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

const serverUrls = {
  github: 'https://github.com',
  forgejo: 'https://git.start9labs.com',
}

function enabled(step, inputs, privateRepository, forge) {
  return Boolean(
    new Function('inputs', 'github', `return (${step.if.slice(3, -2)})`)(
      inputs,
      {
        event: { repository: { private: privateRepository } },
        server_url: serverUrls[forge],
      },
    ),
  )
}

function runStep(step, env, values = {}) {
  const script = step.run.replace(/\$\{\{\s*(.*?)\s*\}\}/g, (_, key) => {
    assert.ok(Object.hasOwn(values, key), key)
    return values[key]
  })
  return spawnSync('bash', ['-e', '-o', 'pipefail', '-c', script], {
    env: { ...process.env, ...env },
    cwd: env.CWD,
    encoding: 'utf8',
  })
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
if [ -f "$HOME/.startos/id.key.pem" ] && [ "$(< "$HOME/.startos/id.key.pem")" = "$DEV_KEY" ]; then
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

  test(`${forge} permanent-version gate ignores start-cli log output`, t => {
    const home = mkdtempSync(join(tmpdir(), 'permanent-version-gate-'))
    t.after(() => rmSync(home, { recursive: true, force: true }))
    const output = join(home, 'output')
    writeFileSync(
      join(home, 'start-cli'),
      `#!/bin/bash
echo 'INFO Renamed legacy key file' >&2
`,
      { mode: 0o755 },
    )
    const check = document('tagAndRelease').jobs.Tag.steps.find(
      step => step.id === 'check',
    )
    const result = runStep(
      check,
      {
        HOME: home,
        PATH: `${home}:${process.env.PATH}`,
        GITHUB_OUTPUT: output,
      },
      {
        'steps.extract.outputs.package_id': 'fixture',
        'steps.extract.outputs.version': '1.0.0:0',
        'inputs.REFERENCE_REGISTRY': 'https://registry.example',
      },
    )
    assert.equal(result.status, 0, result.stderr)
    assert.match(readFileSync(output, 'utf8'), /^skip=false$/m)
  })

  test(`${forge} validates private publication before building`, () => {
    const validation = document('release').jobs.DiscoverMatrix.steps.find(
      step => step.name === 'Validate private publication',
    )
    assert.equal(
      enabled(validation, { RELEASE_REGISTRY: '' }, true, forge),
      false,
    )
    const status = env =>
      runStep(validation, {
        PRIVATE_REPOSITORY: 'false',
        READER_KEYS: '',
        S3_BASE: '',
        ON_GITHUB: String(forge === 'github'),
        HAS_PACKAGES_TOKEN: 'false',
        ...env,
      }).status
    assert.equal(status({}), 0)
    assert.equal(status({ READER_KEYS: 'MCowBQYDK2VwAyEA' }), 1)
    assert.equal(status({ PRIVATE_REPOSITORY: 'true' }), 1)
    assert.equal(
      status({ PRIVATE_REPOSITORY: 'true', HAS_PACKAGES_TOKEN: 'true' }),
      forge === 'github' ? 1 : 0,
    )
    assert.equal(
      status({ PRIVATE_REPOSITORY: 'true', S3_BASE: 'https://bucket.example' }),
      0,
    )
  })

  test(`${forge} private repositories publish with --private and each reader key`, t => {
    const cwd = mkdtempSync(join(tmpdir(), 'private-package-args-'))
    t.after(() => rmSync(cwd, { recursive: true, force: true }))
    const resolve = document('release').jobs.Publish.steps.find(
      step => step.name === 'Resolve private publication arguments',
    )
    const values = { 'steps.version.outputs.package_id': 'private-fixture' }
    const args = env => {
      const result = runStep(
        resolve,
        { CWD: cwd, READER_KEYS: '', ...env },
        values,
      )
      assert.equal(result.status, 0, result.stderr)
      return readFileSync(join(cwd, '.private_args'), 'utf8')
    }
    assert.equal(args({ PRIVATE_REPOSITORY: 'false' }), '')
    assert.equal(
      args({
        PRIVATE_REPOSITORY: 'true',
        READER_KEYS:
          'KEYA\r\n-----BEGIN PUBLIC KEY-----\nKEYB\n-----END PUBLIC KEY-----\n',
      }),
      '--private\n--reader\nKEYA\n--reader\nKEYB\n',
    )
  })

  test(`${forge} hosts oversize public s9pks in Forgejo packages and links them`, t => {
    const cwd = mkdtempSync(join(tmpdir(), 'oversize-s9pk-'))
    t.after(() => rmSync(cwd, { recursive: true, force: true }))
    execFileSync('truncate', ['-s', '2G', join(cwd, 'big.s9pk')])
    writeFileSync(join(cwd, 'small.s9pk'), 'small')
    writeFileSync(
      join(cwd, 'start-cli'),
      `#!/bin/bash\nprintf '%s' '{"releaseNotes":"notes"}'\n`,
      { mode: 0o755 },
    )
    const steps = document('release').jobs.Publish.steps
    const step = name => steps.find(step => step.name === name)
    const base = `${serverUrls[forge]}/api/packages/owner/generic/demo/1.0.0_0`
    const values = {
      'steps.version.outputs.version': '1.0.0:0',
      'steps.version.outputs.package_id': 'demo',
      'github.server_url': serverUrls[forge],
      'github.repository_owner': 'owner',
      'steps.filter_s9pk.outputs.packages_base': base,
      'inputs.S3_S9PKS_BASE_URL': '',
    }
    const env = {
      CWD: cwd,
      PATH: `${cwd}:${process.env.PATH}`,
      GITHUB_OUTPUT: join(cwd, 'output'),
      S3_BASE: '',
      PRIVATE_REPOSITORY: 'false',
      ON_GITHUB: String(forge === 'github'),
      HAS_PACKAGES_TOKEN: 'true',
    }
    const filter = runStep(
      step('Filter s9pk files for GitHub release'),
      env,
      values,
    )
    if (forge === 'github') {
      assert.equal(filter.status, 1)
      return
    }
    assert.equal(filter.status, 0, filter.stderr)
    assert.equal(
      readFileSync(join(cwd, '.package_files'), 'utf8'),
      'big.s9pk\n',
    )
    assert.equal(
      readFileSync(join(cwd, '.gh_release_files'), 'utf8'),
      'small.s9pk\n',
    )
    const changelog = runStep(step('Generate changelog'), env, values)
    assert.equal(changelog.status, 0, changelog.stderr)
    assert.match(
      readFileSync(join(cwd, 'change-log.txt'), 'utf8'),
      new RegExp(
        `## Downloads\\n\\n- \\[big\\.s9pk\\]\\(${base.replace(/[.]/g, '\\.')}/big\\.s9pk\\)`,
      ),
    )
    assert.equal(
      runStep(
        step('Filter s9pk files for GitHub release'),
        { ...env, HAS_PACKAGES_TOKEN: 'false' },
        values,
      ).status,
      1,
    )
  })

  for (const [privateRepository, s3, expected] of [
    [true, 'https://bucket.example', ['Publish to Registry via S3']],
    [
      true,
      '',
      forge === 'github' ? [] : ['Publish to Registry via Forgejo packages'],
    ],
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
          .filter(step => enabled(step, inputs, privateRepository, forge))
          .map(step => step.name),
        expected,
      )
    })
  }
}
