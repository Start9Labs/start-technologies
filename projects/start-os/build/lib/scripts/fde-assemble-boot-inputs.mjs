#!/usr/bin/env node
import { spawn } from 'node:child_process'
import { constants } from 'node:fs'
import { copyFile, mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { withAssembledInitramfs } from './fde-initramfs-generation.mjs'
import { bootProviders } from './fde-boot-inputs.mjs'

if (process.argv.length !== 2)
  throw Error('Usage: fde-assemble-boot-inputs.mjs')
const files = (
  await readFile(new URL('./fde-boot-input-files', import.meta.url), 'utf8')
)
  .trim()
  .split('\n')
const [linux, initrd, osRelease] = files
const directory = await mkdtemp(join(tmpdir(), '.fde-boot-inputs-'))
try {
  const { kernel, osRelease: release } = await bootProviders('/')
  await copyFile(kernel, join(directory, linux), constants.COPYFILE_EXCL)
  await copyFile(release, join(directory, osRelease), constants.COPYFILE_EXCL)
  await withAssembledInitramfs(tmpdir(), async archive => {
    await copyFile(archive, join(directory, initrd), constants.COPYFILE_EXCL)
    await new Promise((resolve, reject) => {
      const child = spawn(
        '/usr/bin/tar',
        ['--format=posix', '-C', directory, '-cf', '-', '--', ...files],
        {
          env: { PATH: '/usr/sbin:/usr/bin:/sbin:/bin', LC_ALL: 'C' },
          stdio: ['ignore', 'inherit', 'inherit'],
        },
      )
      child.once('error', reject)
      child.once('close', (code, signal) =>
        code === 0
          ? resolve()
          : reject(Error(`Boot input export failed: ${code ?? signal}`)),
      )
    })
  })
} finally {
  await rm(directory, { recursive: true })
}
