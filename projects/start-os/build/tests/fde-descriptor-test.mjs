import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import test from 'node:test'

import {
  descriptorFile,
  installBootDescriptor,
} from '../lib/scripts/fde-initramfs-preparation.mjs'

const bytes = Buffer.from('public boot data\n')

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'fde-descriptor-'))
  t.after(() => rm(root, { recursive: true }))
  await mkdir(join(root, 'etc'))
  return root
}

test('descriptor bytes occupy the boot consumer path with private file permissions', async t => {
  const root = await fixture(t)
  assert.equal(descriptorFile, '/etc/startos/fde-boot.json')
  await installBootDescriptor(root, bytes)
  const path = join(root, descriptorFile.slice(1))
  assert.deepEqual(await readFile(path), bytes)
  assert.equal((await lstat(path)).mode & 0o777, 0o600)
  await assert.rejects(
    installBootDescriptor(root, Buffer.from('replacement')),
    { code: 'EEXIST' },
  )
  assert.deepEqual(await readFile(path), bytes)
})

test('redirected generated ancestors and occupied leaf symlinks reject without external writes', async t => {
  const root = await fixture(t)
  const outside = await fixture(t)
  const directory = dirname(descriptorFile.slice(1))
  await symlink(outside, join(root, directory))
  await assert.rejects(
    installBootDescriptor(root, bytes),
    /Unsupported generated directory/,
  )
  await rm(join(root, directory))
  await mkdir(join(root, directory))
  const target = join(outside, 'sentinel')
  await writeFile(target, 'unchanged')
  await symlink(target, join(root, descriptorFile.slice(1)))
  await assert.rejects(installBootDescriptor(root, bytes), { code: 'EEXIST' })
  assert.equal(await readFile(target, 'utf8'), 'unchanged')
  await rm(join(root, 'etc'), { recursive: true })
  await symlink(outside, join(root, 'etc'))
  await assert.rejects(
    installBootDescriptor(root, bytes),
    /Unsupported generated directory/,
  )
})

test('missing descriptor data rejects before preparing any target directory', async t => {
  const root = await fixture(t)
  await assert.rejects(
    installBootDescriptor(root, Buffer.alloc(0)),
    /Missing boot descriptor/,
  )
  await assert.rejects(lstat(join(root, dirname(descriptorFile.slice(1)))), {
    code: 'ENOENT',
  })
})

test('native cpio and gzip retain descriptor bytes and mode without a second archive codec', async t => {
  const root = await fixture(t)
  const extracted = await fixture(t)
  await installBootDescriptor(root, bytes)
  const cpio = execFileSync(
    '/usr/bin/cpio',
    ['--null', '--create', '--format=newc', '--quiet'],
    {
      cwd: root,
      input: Buffer.from(`${descriptorFile.slice(1)}\0`),
    },
  )
  const gzip = execFileSync('/usr/bin/gzip', ['-n', '-c'], { input: cpio })
  const restored = execFileSync('/usr/bin/gzip', ['-d', '-c'], { input: gzip })
  execFileSync(
    '/usr/bin/cpio',
    ['--extract', '--make-directories', '--no-absolute-filenames', '--quiet'],
    {
      cwd: extracted,
      input: restored,
    },
  )
  const path = join(extracted, descriptorFile.slice(1))
  assert.deepEqual(await readFile(path), bytes)
  assert.equal((await lstat(path)).mode & 0o777, 0o600)
})
