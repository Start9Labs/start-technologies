import assert from 'node:assert/strict'
import { join } from 'node:path'
import { test } from 'node:test'
import { REPO, fixture } from './fixtures.mjs'

const SHA = 'a'.repeat(40)
const HEAD = 'b'.repeat(40)

for (const [name, run, expected] of [
  [
    'a build run reports its head commit',
    {
      path: '.github/workflows/build-products.yaml',
      head_sha: SHA,
      display_title: 'fix: x',
    },
    SHA,
  ],
  [
    'a StartOS publish run reports the build commit its title names',
    {
      path: '.github/workflows/startos-iso.yaml',
      head_sha: HEAD,
      display_title: `StartOS alpha publish of ${SHA}`,
    },
    SHA,
  ],
  [
    'a publish run without a commit in its title is refused',
    {
      path: '.github/workflows/startos-iso.yaml',
      head_sha: HEAD,
      display_title: 'Debian-based ISO and SquashFS',
    },
    null,
  ],
]) {
  test(`gha-run-commit: ${name}`, t => {
    const f = fixture(t)
    f.write('run.json', JSON.stringify(run))
    f.nodeExecutable(
      'gh',
      `import { readFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
const filter = process.argv[process.argv.indexOf('--jq') + 1]
const result = spawnSync('jq', ['-r', filter], { input: readFileSync(${JSON.stringify(join(f.root, 'run.json'))}), encoding: 'utf8' })
process.stdout.write(result.stdout)
process.exit(result.status)
`,
    )
    const result = f.run(
      join(REPO, 'scripts/gha-run-commit.sh'),
      ['Start9Labs/start-technologies', '42'],
      { env: { ...f.env, PATH: `${f.root}:${f.env.PATH}` }, encoding: 'utf8' },
    )
    if (expected === null) assert.notEqual(result.status, 0)
    else {
      assert.equal(result.status, 0, result.stderr)
      assert.equal(result.stdout.trim(), expected)
    }
  })
}
