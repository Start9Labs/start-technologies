import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'

const require = createRequire(import.meta.url)
const yaml = require('js-yaml')
const action = yaml.load(
  readFileSync(
    new URL('../configure-registry-proxy/action.yml', import.meta.url),
    'utf8',
  ),
)
const step = action.runs.steps.find(
  ({ name }) => name === 'Configure registry proxy',
)
const torStep = action.runs.steps.find(({ name }) => name === 'Start Tor')

test('Tor starts only when the repository opts in', () => {
  assert.equal(torStep.if, "${{ inputs.tor == 'true' }}")
  assert.equal(
    step.env.PROXY,
    "${{ inputs.tor == 'true' && 'socks5h://127.0.0.1:9050' || inputs.proxy }}",
  )
})

for (const existing of [
  '',
  'registry: https://registry.example',
  'registry: https://registry.example\nproxy: "socks5h://stale.example:9050"',
]) {
  test(`reapplying a proxy leaves one proxy in ${JSON.stringify(existing)}`, t => {
    const home = mkdtempSync(join(tmpdir(), 'registry-proxy-test-'))
    t.after(() => rmSync(home, { recursive: true, force: true }))
    const config = join(home, '.startos', 'config.yaml')
    if (existing) {
      mkdirSync(join(home, '.startos'))
      writeFileSync(config, existing)
    }
    const proxy = 'socks5h://registry-proxy.example:9050'
    const configure = () =>
      execFileSync('bash', ['-e', '-o', 'pipefail', '-c', step.run], {
        env: { ...process.env, HOME: home, PROXY: proxy },
        stdio: 'pipe',
      })

    configure()
    const first = readFileSync(config, 'utf8')
    configure()
    const second = readFileSync(config, 'utf8')

    assert.equal(second, first)
    assert.deepEqual(yaml.load(second), {
      ...(existing ? { registry: 'https://registry.example' } : {}),
      proxy,
    })
  })
}

test('an unset proxy removes a proxy an earlier job configured', t => {
  const home = mkdtempSync(join(tmpdir(), 'registry-proxy-test-'))
  t.after(() => rmSync(home, { recursive: true, force: true }))
  const config = join(home, '.startos', 'config.yaml')
  mkdirSync(join(home, '.startos'))
  writeFileSync(
    config,
    'registry: https://registry.example\nproxy: "socks5h://stale.example:9050"\n',
  )
  execFileSync('bash', ['-e', '-o', 'pipefail', '-c', step.run], {
    env: { ...process.env, HOME: home, PROXY: '' },
    stdio: 'pipe',
  })
  assert.deepEqual(yaml.load(readFileSync(config, 'utf8')), {
    registry: 'https://registry.example',
  })
})
