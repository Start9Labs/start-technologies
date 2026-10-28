import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createRequire } from 'node:module'
import { test } from 'node:test'

const require = createRequire(import.meta.url)
const yaml = require('js-yaml')
const action = yaml.load(
  readFileSync(
    new URL('../install-start-cli/action.yml', import.meta.url),
    'utf8',
  ),
)
const step = action.runs.steps[0]

test('only a GitHub.com caller token is eligible for the GitHub download', () => {
  assert.equal(
    step.env.INSTALL_GITHUB_TOKEN,
    "${{ github.server_url == 'https://github.com' && github.token || '' }}",
  )
})

for (const token of ['', 'github-test-token']) {
  test(`start-cli download ${token ? 'uses GitHub credentials' : 'works without sending a foreign token'}`, t => {
    const dir = mkdtempSync(join(tmpdir(), 'install-cli-test-'))
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    const executable = (name, text) =>
      writeFileSync(join(dir, name), `#!/bin/bash\n${text}\n`, { mode: 0o755 })
    const log = join(dir, 'requests')
    executable(
      'curl',
      `printf '%s\\n' "$*" >> "$CURL_LOG"
if [[ "$*" == *api.github.com* ]]; then
  printf '[{"assets":[{"name":"start-cli_%s-%s","browser_download_url":"https://github.com/asset"}]}]' "$(uname -m)" "$(uname -s | tr '[:upper:]' '[:lower:]')"
fi`,
    )
    executable('sudo', 'exit 0')
    executable('rm', 'exit 0')
    executable('start-cli', 'echo test-version')
    execFileSync('bash', ['-e', '-o', 'pipefail', '-c', step.run], {
      env: {
        ...process.env,
        PATH: `${dir}:${process.env.PATH}`,
        INSTALL_GITHUB_TOKEN: token,
        CURL_LOG: log,
      },
    })
    const requests = readFileSync(log, 'utf8').trim().split('\n')
    assert.equal(requests.length, 2)
    for (const request of requests) {
      assert.equal(request.includes('Authorization: token'), Boolean(token))
      if (token) assert.ok(request.includes(token))
    }
  })
}
