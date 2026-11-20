import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO } from './fixtures.mjs'
import { artifactFixture } from './gha-artifact-fixture.mjs'

function download(t, runs, glob) {
  const f = artifactFixture(t, runs)
  const result = f.run(
    'bash',
    [
      join(REPO, 'scripts/gha-artifact-download.sh'),
      'Start9Labs/start-technologies',
      '7',
      glob,
      'out',
    ],
    { env: f.env, encoding: 'utf8' },
  )
  const dir = join(f.root, 'out')
  return { result, dir, files: () => readdirSync(dir).sort() }
}

test('saves each unexpired matching artifact of the run under its name', t => {
  const { result, dir, files } = download(
    t,
    [
      {
        id: 7,
        artifacts: [
          'start-cli-0.4.0-aaaaaaa_x86_64.deb',
          'start-cli-0.4.0-aaaaaaa_aarch64.deb',
          { name: 'start-cli-0.4.0-aaaaaaa_riscv64.deb', expired: true },
          'start-tunnel-0.4.0-aaaaaaa_x86_64.deb',
          'start-cli_x86_64-unknown-linux-musl',
        ],
      },
      { id: 8, artifacts: ['start-cli-0.4.0-bbbbbbb_x86_64.deb'] },
    ],
    'start-cli-*.deb',
  )
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.deepEqual(files(), [
    'start-cli-0.4.0-aaaaaaa_aarch64.deb',
    'start-cli-0.4.0-aaaaaaa_x86_64.deb',
  ])
  assert.equal(
    readFileSync(join(dir, 'start-cli-0.4.0-aaaaaaa_x86_64.deb'), 'utf8'),
    'start-cli-0.4.0-aaaaaaa_x86_64.deb contents\n',
  )
})

test('fails when the run holds no matching artifact', t => {
  const { result, files } = download(
    t,
    [{ id: 7, artifacts: ['x86_64.squashfs'] }],
    'startos-*_x86_64.squashfs',
  )
  assert.notEqual(result.status, 0)
  assert.match(result.stderr, /no unexpired artifact matching/)
  assert.deepEqual(files(), [])
})
