import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdtempSync, symlinkSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'

for (const [action, forge, expected] of [
  ['forgejo-release', 'forgejo', /ENOENT/],
  ['sync-next', 'forgejo', /git config user.name forgejo-actions\[bot\]/],
  ['sync-next', 'gitea', /git config user.name gitea-actions\[bot\]/],
  ['sync-next', 'github', /git config user.name github-actions\[bot\]/],
]) {
  test(`${action} executes through ${forge}'s symlinked action path`, t => {
    const dir = mkdtempSync(join(tmpdir(), 'action-entrypoint-'))
    t.after(() => rmSync(dir, { recursive: true, force: true }))
    const actionPath = join(dir, 'action')
    symlinkSync(
      fileURLToPath(new URL(`../${action}`, import.meta.url)),
      actionPath,
    )
    const result = spawnSync(process.execPath, [join(actionPath, 'main.mjs')], {
      cwd: dir,
      encoding: 'utf8',
      env: {
        ...process.env,
        FORGEJO_ACTIONS: String(forge === 'forgejo'),
        GITEA_ACTIONS: String(forge === 'gitea'),
        GITHUB_SERVER_URL: 'https://forge.example',
        GITHUB_REPOSITORY: 'owner/repo',
        RELEASE_BODY_PATH: join(dir, 'missing-notes'),
      },
    })
    assert.notEqual(result.status, 0)
    assert.match(result.stderr, expected)
  })
}
