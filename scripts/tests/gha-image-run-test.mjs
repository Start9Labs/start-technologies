import assert from 'node:assert/strict'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO, fixture } from './fixtures.mjs'

// A higher id is a newer run, within artifact retention.
const run = (id, artifacts, overrides = {}) => ({
  id,
  created_at: new Date(Date.now() - (10 - id) * 3600e3)
    .toISOString()
    .replace(/\.\d+Z$/, 'Z'),
  head_branch: 'master',
  conclusion: 'success',
  artifacts: artifacts.map(name => ({ name, expired: false })),
  ...overrides,
})
const IMAGE = 'x86_64.squashfs'

function select(t, workflows) {
  const f = fixture(t)
  f.write('workflows.json', JSON.stringify(workflows))
  f.nodeExecutable(
    'gh',
    `import { readFileSync } from 'node:fs'
const workflows = JSON.parse(readFileSync(${JSON.stringify(join(f.root, 'workflows.json'))}))
const args = process.argv.slice(2)
const url = args.find(arg => arg.startsWith('repos/'))
const workflow = url.match(/workflows\\/([^/]+)\\/runs/)
if (workflow) {
  const page = Number(new URL('https://x/' + url).searchParams.get('page'))
  const runs = workflows[workflow[1]]?.[page - 1] ?? []
  console.log(JSON.stringify({ workflow_runs: runs.map(({ artifacts, ...run }) => run) }))
} else {
  const id = Number(url.match(/runs\\/(\\d+)\\/artifacts/)[1])
  const run = Object.values(workflows).flat(2).find(run => run.id === id)
  console.log(JSON.stringify([{ artifacts: run.artifacts }]))
}
`,
  )
  return f.run(
    'bash',
    [
      join(REPO, 'scripts/gha-image-run.sh'),
      'Start9Labs/start-technologies',
      'master',
      'x86_64',
    ],
    {
      env: { ...f.env, PATH: `${f.root}:${f.env.PATH}` },
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
    'continues beyond a page with no successful builds for the branch',
    {
      'build-products.yaml': [
        [run(3, [IMAGE], { head_branch: 'integration/other' })],
        [run(2, [IMAGE])],
      ],
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

test('image run selection: runs past artifact retention end the search', t => {
  const result = select(t, {
    'build-products.yaml': [
      [
        run(1, [IMAGE], {
          created_at: new Date(Date.now() - 20 * 86400e3).toISOString(),
        }),
      ],
    ],
  })
  assert.notEqual(result.status, 0)
})
