import assert from 'node:assert/strict'
import {
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  readdir,
  rm,
  symlink,
  unlink,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { basename, join, resolve } from 'node:path'
import test from 'node:test'

import { bootProviders } from '../lib/scripts/fde-boot-inputs.mjs'

const root = resolve(process.argv[2] ?? '')
if (!process.argv[2]) throw Error('Requires a genuine extracted public OS root')
const genuine = await bootProviders(root)

test('selected boot providers belong to the genuine single-kernel source', async () => {
  const kernels = (await readdir(join(root, 'boot'))).filter(name =>
    name.startsWith('vmlinuz-'),
  )
  assert.equal(kernels.length, 1)
  assert.equal(genuine.kernel, join(root, 'boot', kernels[0]))
  assert.equal(genuine.osRelease, join(root, 'usr/lib/os-release'))
  assert.ok((await lstat(genuine.kernel)).size > 0)
  assert.ok((await lstat(genuine.osRelease)).size > 0)
})

test('missing, redirected and ambiguous boot providers reject', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'fde-boot-providers-'))
  const version = basename(genuine.kernel).slice('vmlinuz-'.length)
  try {
    await mkdir(join(directory, 'usr/lib/modules', version), {
      recursive: true,
    })
    await mkdir(join(directory, 'boot'))
    await copyFile(genuine.osRelease, join(directory, 'usr/lib/os-release'))
    await assert.rejects(bootProviders(directory))
    const kernel = join(directory, 'boot', basename(genuine.kernel))
    await symlink(genuine.kernel, kernel)
    await assert.rejects(bootProviders(directory))
    await unlink(kernel)
    await copyFile(genuine.kernel, kernel)
    await bootProviders(directory)
    await unlink(join(directory, 'usr/lib/os-release'))
    await symlink(genuine.osRelease, join(directory, 'usr/lib/os-release'))
    await assert.rejects(bootProviders(directory))
    await unlink(join(directory, 'usr/lib/os-release'))
    await copyFile(genuine.osRelease, join(directory, 'usr/lib/os-release'))
    await mkdir(join(directory, 'usr/lib/modules/other'))
    await assert.rejects(bootProviders(directory))
  } finally {
    await rm(directory, { recursive: true })
  }
})
