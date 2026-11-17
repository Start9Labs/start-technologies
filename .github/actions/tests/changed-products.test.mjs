import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO } from '../../../scripts/tests/fixtures.mjs'
import {
  classify,
  PRODUCTS,
} from '../../../.github/scripts/changed-products.mjs'

const ALL = Object.keys(PRODUCTS)
const DEBS = ['start-cli', 'start-registry', 'start-tunnel']

for (const [path, push, pullRequest = push, osImage = false] of [
  ['shared-libs/crates/start-core/src/lib.rs', ALL],
  ['Cargo.lock', ALL],
  ['build/env/check-platform.sh', ALL, ALL, true],
  ['debian/build.sh', ALL, ALL, true],
  ['.github/workflows/build-products.yaml', ALL, ALL, true],
  ['.github/scripts/changed-products.mjs', ALL],
  ['projects/start-cli/src/main.rs', ['start-cli']],
  ['projects/start-registry/Cargo.toml', ['start-registry']],
  ['projects/start-tunnel/web/src/main.ts', ['start-tunnel']],
  ['shared-libs/ts-modules/shared/src/index.ts', ['start-os', 'start-tunnel']],
  ['package-lock.json', ['start-os', 'start-tunnel']],
  ['package.json', ['start-os', 'start-tunnel'], undefined, true],
  ['projects/start-sdk/lib/index.ts', ['start-os']],
  [
    'projects/start-os/build/image-recipe/build.sh',
    ['start-os'],
    undefined,
    true,
  ],
  ['projects/start-os/startd.service', ['start-os'], undefined, true],
  ['projects/start-os/src/main.rs', ['start-os']],
  ['projects/start-os/docs/src/overview.md', []],
  ['projects/start-os/release-notes/0.4.1.md', ['start-os'], undefined, true],
  ['projects/start-cli/changelog/patch-fixed-x.md', ['start-cli'], []],
  ['projects/start-tunnel/release-notes/1.2.0.md', ['start-tunnel'], []],
  ['scripts/manage-release.sh', DEBS, []],
  ['scripts/changelog.mjs', ALL, ['start-os'], true],
  ['.prettierrc.json', ALL, ['start-os'], true],
  ['AGENTS.md', []],
  ['projects/start-wrt/backend/ctrl/src/lib.rs', []],
]) {
  test(`${path} rebuilds the products whose build reads it`, () => {
    for (const [event, expected] of [
      ['push', push],
      ['pull_request', pullRequest],
    ]) {
      const result = classify([path], event)
      assert.deepEqual(result.affected, expected, event)
      assert.equal(result.osImage, osImage && expected.includes('start-os'))
    }
  })
}

test('the script prints step outputs for the changed paths on stdin', () => {
  const script = join(REPO, '.github/scripts/changed-products.mjs')
  const run = (args, input) =>
    spawnSync(process.execPath, [script, ...args], { input, encoding: 'utf8' })
  const result = run(
    ['--event', 'pull_request'],
    'projects/start-cli/src/main.rs\nprojects/start-registry/src/main.rs\n',
  )
  assert.equal(result.status, 0, result.stderr)
  assert.equal(
    result.stdout,
    [
      'start-os=false',
      'start-cli=true',
      'start-registry=true',
      'start-tunnel=false',
      'os-image=false',
      'products=start-cli start-registry',
      '',
    ].join('\n'),
  )
  const all = run(['--all'], '')
  assert.equal(all.status, 0, all.stderr)
  assert.match(
    all.stdout,
    /^products=start-os start-cli start-registry start-tunnel$/m,
  )
  assert.match(all.stdout, /^os-image=true$/m)
  assert.notEqual(run(['--event', 'workflow_dispatch'], '').status, 0)
})
