import assert from 'node:assert/strict'
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createRequire } from 'node:module'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'

const require = createRequire(import.meta.url)
const yaml = require('js-yaml')
const workflow = yaml.load(
  readFileSync(
    new URL('../../workflows/live-docs-guard.yml', import.meta.url),
    'utf8',
  ),
)
const script = workflow.jobs['docs-only'].steps[0].run

function check(t, files, expected = files.length) {
  const cwd = mkdtempSync(join(tmpdir(), 'live-docs-guard-'))
  t.after(() => rmSync(cwd, { recursive: true, force: true }))
  writeFileSync(
    join(cwd, 'gh'),
    `#!/bin/bash
case "$*" in
  */files*) printf '%s' "$FILES" ;;
  *) printf '%s' "$EXPECTED" ;;
esac
`,
    { mode: 0o755 },
  )
  return spawnSync('bash', ['-c', script], {
    cwd,
    encoding: 'utf8',
    env: {
      ...process.env,
      PATH: `${cwd}:${process.env.PATH}`,
      GITHUB_REPOSITORY: 'owner/repo',
      PR: '1',
      FILES: JSON.stringify([files]),
      EXPECTED: String(expected),
    },
  })
}

for (const file of [
  'projects/start-sdk/docs/src/project-structure.md',
  'projects/start-docs/theme/book.js',
  'projects/new-product/docs/src/index.md',
]) {
  test(`published book path ${file} is accepted`, t => {
    const result = check(t, [{ filename: file }])
    assert.equal(result.status, 0, result.stdout + result.stderr)
  })
}

for (const file of [
  'projects/start-sdk/docs/package-template/package.json',
  'projects/start-sdk/docs/gitea-workflows/build.yml',
  'scripts/generate-forgejo-workflows.cjs',
]) {
  test(`code path ${file} is rejected`, t => {
    const result = check(t, [{ filename: file }])
    assert.equal(result.status, 1)
    assert.match(result.stdout, /may only change published book files/)
    assert.ok(result.stdout.includes(file))
  })
}

for (const [from, to] of [
  [
    'projects/start-sdk/docs/gitea-workflows/build.yml',
    'projects/start-sdk/docs/src/build.md',
  ],
  [
    'projects/start-sdk/docs/src/build.md',
    'projects/start-sdk/docs/gitea-workflows/build.yml',
  ],
]) {
  test(`renaming ${from} to ${to} is rejected`, t => {
    const result = check(t, [{ filename: to, previous_filename: from }])
    assert.equal(result.status, 1)
    assert.match(result.stdout, /gitea-workflows\/build.yml/)
  })
}

test('an incomplete file list is rejected', t => {
  const result = check(
    t,
    [{ filename: 'projects/start-sdk/docs/src/project-structure.md' }],
    2,
  )
  assert.equal(result.status, 1)
  assert.match(result.stdout, /changed-file list is incomplete/)
})
