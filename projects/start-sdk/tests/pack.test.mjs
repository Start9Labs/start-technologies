import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import test from 'node:test'

const packer = resolve('pack.mjs')
for (const [enabled, packed, packStatus, inspectStatus, succeeds] of [
  [false, undefined, 0, 0, true],
  [true, true, 0, 0, true],
  [true, undefined, 0, 0, false],
  [true, false, 0, 0, false],
  [true, true, 1, 0, false],
  [true, true, 0, 1, false],
]) {
  test(`packed volume capability ${enabled}/${packed}, pack ${packStatus}, inspect ${inspectStatus}`, t => {
    const directory = mkdtempSync(join(tmpdir(), 'sdk-pack-'))
    t.after(() => rmSync(directory, { recursive: true, force: true }))
    mkdirSync(join(directory, 'javascript'))
    mkdirSync(join(directory, 'bin'))
    writeFileSync(
      join(directory, 'javascript/index.js'),
      `exports.manifest=${JSON.stringify({ sharedVolumes: enabled })}`,
    )
    writeFileSync(
      join(directory, 'bin/start-cli'),
      `#!${process.execPath}
const fs=require('node:fs')
const args=process.argv.slice(2)
fs.appendFileSync('calls',JSON.stringify(args)+'\\n')
if(args[1]==='pack') {
  fs.writeFileSync(args[args.indexOf('-o')+1],JSON.stringify(${JSON.stringify({ sharedVolumes: packed })}))
  process.exit(${packStatus})
}
if(args[1]==='inspect') {
  if(${inspectStatus}) process.exit(${inspectStatus})
  process.stdout.write(fs.readFileSync(args[2]))
} else process.exit(1)
`,
      { mode: 0o755 },
    )
    const result = spawnSync(
      process.execPath,
      [packer, 'volume.s9pk', '--arch=x86_64'],
      {
        cwd: directory,
        encoding: 'utf8',
        env: {
          ...process.env,
          PATH: `${join(directory, 'bin')}:${process.env.PATH}`,
        },
      },
    )
    assert.equal(result.status === 0, succeeds, result.stderr)
    assert.equal(existsSync(join(directory, 'volume.s9pk')), succeeds)
    const calls = readFileSync(join(directory, 'calls'), 'utf8')
      .trim()
      .split('\n')
      .map(JSON.parse)
    assert.deepEqual(calls[0], [
      's9pk',
      'pack',
      '--arch=x86_64',
      '-o',
      'volume.s9pk',
    ])
    assert.equal(calls.length, enabled && !packStatus ? 2 : 1)
    if (enabled && !packStatus)
      assert.deepEqual(calls[1], [
        's9pk',
        'inspect',
        'volume.s9pk',
        'cat',
        'manifest.json',
      ])
    if (enabled && !packStatus && !inspectStatus && packed !== true)
      assert.match(result.stderr, /did not preserve sharedVolumes/)
  })
}
