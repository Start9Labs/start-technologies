import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { lstat, mkdtemp, readFile, readdir, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import test from 'node:test'

import { prepareNativeHooks } from '../lib/scripts/fde-initramfs-hooks.mjs'

assert.ok(process.argv[2], 'Provide a genuine complete OS source root')
const source = resolve(process.argv[2])
const installed = join(source, 'usr/share/initramfs-tools/hooks')
const excluded = ['kms', 'live', 'lvm2', 'thin-provisioning-tools']

async function identities() {
  return Promise.all(
    excluded.map(async name => {
      const path = join(installed, name)
      assert.ok((await lstat(path)).isFile())
      return [
        name,
        createHash('sha256')
          .update(await readFile(path))
          .digest('hex'),
      ]
    }),
  )
}

test('the installed storage/live/graphics producers remain intact outside the bootstrap profile', async t => {
  const parent = await mkdtemp(join(tmpdir(), 'fde-bootstrap-hooks-'))
  t.after(() => rm(parent, { recursive: true, force: true }))
  const before = await identities()
  const entries = await readdir(installed)
  const target = join(parent, 'hooks')
  await prepareNativeHooks(source, target)
  const names = new Set(await readdir(target))
  for (const name of [
    'fsck',
    'kmod',
    'klibc-utils',
    'thermal',
    'zz-busybox',
    'udev',
    'dmsetup',
    'btrfs',
  ]) {
    assert.ok(names.delete(name), `Missing bootstrap provider: ${name}`)
  }
  names.delete('intel_microcode')
  names.delete('amd64_microcode')
  assert.deepEqual([...names], [])
  for (const name of excluded) {
    await assert.rejects(lstat(join(target, name)), { code: 'ENOENT' })
  }
  assert.deepEqual(await identities(), before)
  assert.deepEqual(await readdir(installed), entries)
})
