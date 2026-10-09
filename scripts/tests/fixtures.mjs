import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

export const REPO = fileURLToPath(new URL('../../', import.meta.url))

export function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'release-tooling-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const env = {
    ...process.env,
    GIT_CONFIG_COUNT: '0',
    GIT_CONFIG_NOSYSTEM: '1',
  }
  function run(command, args, options = {}) {
    const result = spawnSync(command, args, {
      cwd: root,
      env,
      maxBuffer: Infinity,
      ...options,
    })
    assert.ifError(result.error)
    return result
  }
  function git(...args) {
    const result = run('git', args, { encoding: 'utf8' })
    assert.equal(result.status, 0, result.stderr)
    return result.stdout
  }
  function write(name, data) {
    const path = join(root, name)
    mkdirSync(dirname(path), { recursive: true })
    writeFileSync(path, data)
    return path
  }
  function nodeExecutable(name, source) {
    const path = write(`${name}.mjs`, `#!/usr/bin/env node\n${source}`)
    chmodSync(path, 0o755)
    symlinkSync(basename(path), join(root, name))
    return path
  }
  return { root, env, run, git, write, nodeExecutable }
}
