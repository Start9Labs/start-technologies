import assert from 'node:assert/strict'
import {
  chmod,
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

import { admitBootstrapConfiguration } from '../lib/scripts/fde-initramfs-configuration.mjs'
import { runSource } from '../lib/scripts/fde-reviewed-source.mjs'

async function context(t) {
  const root = await mkdtemp(join(tmpdir(), 'fde-configuration-case-'))
  t.after(() => rm(root, { recursive: true }))
  await mkdir(join(root, 'etc/udev'), { recursive: true })
  await writeFile(join(root, 'etc/fstab'), '')
  await chmod(join(root, 'etc/fstab'), 0o644)
  await symlink('/proc/mounts', join(root, 'etc/mtab'))
  await copyFile('/etc/udev/udev.conf', join(root, 'etc/udev/udev.conf'))
  await chmod(
    join(root, 'etc/udev/udev.conf'),
    (await lstat('/etc/udev/udev.conf')).mode & 0o7777,
  )
  return root
}

async function references() {
  return (await readdir(tmpdir()))
    .filter(name => name.startsWith('.fde-configuration-'))
    .sort()
}

test('native source configuration and generated tables admit without changing the candidate', async t => {
  const root = await context(t)
  const before = await references()
  const original = await runSource('/usr/bin/find', [
    root,
    '-printf',
    '%P %m %l\\0',
  ])
  const mask = process.umask(0o077)
  try {
    await admitBootstrapConfiguration(root, new Set())
  } finally {
    process.umask(mask)
  }
  assert.equal((await lstat(join(root, 'etc/fstab'))).mode & 0o777, 0o644)
  assert.equal((await readFile(join(root, 'etc/fstab'))).length, 0)
  assert.deepEqual(
    await readFile(join(root, 'etc/udev/udev.conf')),
    await readFile('/etc/udev/udev.conf'),
  )
  assert.equal(
    (await runSource('/usr/bin/find', [root, '-printf', '%P %m %l\\0'])).stdout,
    original.stdout,
  )
  assert.deepEqual(await references(), before)
})

for (const [name, mutate, pattern] of [
  [
    'changed native udev options',
    root =>
      writeFile(join(root, 'etc/udev/udev.conf'), 'resolve_names=early\n'),
    /Command failed/,
  ],
  [
    'injected NSS selection',
    root =>
      writeFile(join(root, 'etc/nsswitch.conf'), 'passwd: injected_provider\n'),
    /Command failed|ENOENT/,
  ],
  [
    'changed generated fstab',
    root =>
      writeFile(
        join(root, 'etc/fstab'),
        'UUID=unexpected /usr auto defaults 0 0\n',
      ),
    /Command failed/,
  ],
  [
    'redirected native mtab',
    async root => {
      await rm(join(root, 'etc/mtab'))
      await symlink('/outside', join(root, 'etc/mtab'))
    },
    /Generated filesystem table differs/,
  ],
  [
    'unknown runtime configuration',
    root =>
      writeFile(
        join(root, 'etc/fde-unadmitted-runtime.conf'),
        'not source data',
      ),
    /ENOENT/,
  ],
  [
    'redirected configuration file',
    async root => {
      await rm(join(root, 'etc/udev/udev.conf'))
      await symlink('/etc/udev/udev.conf', join(root, 'etc/udev/udev.conf'))
    },
    /Unsupported bootstrap configuration type/,
  ],
  [
    'changed configuration permissions',
    root => chmod(join(root, 'etc/udev/udev.conf'), 0o777),
    /permissions differ/,
  ],
]) {
  test(`${name} rejects before runtime configuration admission`, async t => {
    const root = await context(t)
    await mutate(root)
    const before = await references()
    await assert.rejects(admitBootstrapConfiguration(root, new Set()), pattern)
    assert.deepEqual(await references(), before)
  })
}
