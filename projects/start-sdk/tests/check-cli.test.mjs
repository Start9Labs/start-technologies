import assert from 'node:assert/strict'
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  rmSync,
  symlinkSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { resolve, join } from 'node:path'
import { spawnSync } from 'node:child_process'
import test from 'node:test'

const checker = resolve('check-cli.mjs')
for (const [sharedVolumes, version, succeeds, linked = false] of [
  [false, '2.4.2', true],
  [true, '2.5.2', false],
  [true, '2.6.0', false],
  [true, '2.7.0', true],
  [true, '3.0.0', true],
  [true, '2.6.0', false, true],
]) {
  test(`volume capability ${sharedVolumes} with CLI ${version}, linked ${linked}`, t => {
    const directory = mkdtempSync(join(tmpdir(), 'sdk-cli-'))
    t.after(() => rmSync(directory, { recursive: true, force: true }))
    mkdirSync(join(directory, 'javascript'))
    mkdirSync(join(directory, 'bin'))
    writeFileSync(
      join(directory, 'javascript/index.js'),
      `exports.manifest = ${JSON.stringify({ sharedVolumes })}`,
    )
    writeFileSync(
      join(directory, 'bin/start-cli'),
      `#!/bin/sh\nprintf 'start-cli ${version}\\n'\n`,
      { mode: 0o755 },
    )
    const entrypoint = linked ? join(directory, 'check-cli.mjs') : checker
    if (linked) symlinkSync(checker, entrypoint)
    const result = spawnSync(process.execPath, [entrypoint], {
      cwd: directory,
      encoding: 'utf8',
      env: {
        ...process.env,
        PATH: `${join(directory, 'bin')}:${process.env.PATH}`,
      },
    })
    assert.equal(result.status === 0, succeeds, result.stderr)
    if (!succeeds)
      assert.match(result.stderr, /Volume packages require start-cli 2\.7\.0/)
  })
}
