import assert from 'node:assert/strict'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO } from './fixtures.mjs'
import { artifactFixture } from './gha-artifact-fixture.mjs'

const run = (id, artifacts, overrides = {}) => ({
  id,
  created_at: new Date(Date.now() - (10 - id) * 3600e3)
    .toISOString()
    .replace(/\.\d+Z$/, 'Z'),
  head_branch: 'master',
  conclusion: 'success',
  artifacts,
  ...overrides,
})
const IMAGE = 'x86_64.squashfs'

function select(t, workflows) {
  const runs = Object.entries(workflows).flatMap(([workflow, pages]) =>
    pages.flat().map(run => ({
      path: `.github/workflows/${workflow}`,
      ...run,
    })),
  )
  const f = artifactFixture(t, runs)
  return f.run(
    'bash',
    [
      join(REPO, 'scripts/gha-image-run.sh'),
      'Start9Labs/start-technologies',
      'master',
      'x86_64',
    ],
    {
      env: f.env,
      encoding: 'utf8',
    },
  )
}

for (const [name, workflows, expected] of [
  [
    'skips a newer successful product-only build',
    {
      'build-products.yaml': [
        [run(3, ['start-cli_x86_64.deb']), run(2, [IMAGE])],
      ],
    },
    '2',
  ],
  [
    'compares eligible runs across both workflows',
    {
      'startos-iso.yaml': [[run(1, [IMAGE])]],
      'build-products.yaml': [[run(2, [IMAGE])]],
    },
    '2',
  ],
  [
    'ignores expired and other-platform images',
    {
      'startos-iso.yaml': [
        [run(3, [], { artifacts: [{ name: IMAGE, expired: true }] })],
      ],
      'build-products.yaml': [[run(2, ['aarch64.squashfs']), run(1, [IMAGE])]],
    },
    '1',
  ],
  [
    'images from other branches do not hide the requested branch',
    {
      'build-products.yaml': [
        [run(3, [IMAGE], { head_branch: 'integration/other' })],
        [run(2, [IMAGE])],
      ],
    },
    '2',
  ],
  [
    'unrelated workflows cannot supply a StartOS image',
    {
      'other.yaml': [[run(3, [IMAGE])]],
      'build-products.yaml': [[run(2, [IMAGE])]],
    },
    '2',
  ],
  [
    'fails when no requested image remains',
    { 'build-products.yaml': [[run(1, ['start-cli_x86_64.deb'])]] },
    null,
  ],
]) {
  test(`image run selection: ${name}`, t => {
    const result = select(t, workflows)
    if (expected === null) assert.notEqual(result.status, 0)
    else {
      assert.equal(result.status, 0, result.stdout + result.stderr)
      assert.equal(result.stdout.trim(), expected)
    }
  })
}

test('image run selection: an older rerun can hold an unexpired image', t => {
  const result = select(t, {
    'build-products.yaml': [
      [
        run(1, [IMAGE], {
          created_at: new Date(Date.now() - 20 * 86400e3).toISOString(),
          run_attempt: 2,
        }),
      ],
    ],
  })
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.equal(result.stdout.trim(), '1')
})
