#!/usr/bin/env node
import { constants } from 'node:fs'
import { copyFile, open, readFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { resolve } from 'node:path'

import { withAssembledInitramfs } from './fde-initramfs-generation.mjs'

if (process.argv.length !== 4)
  throw Error(
    'Usage: fde-assemble-initramfs.mjs <private-output-file> <public-boot-descriptor>',
  )
const output = resolve(process.argv[2])
await withAssembledInitramfs(
  tmpdir(),
  await readFile(resolve(process.argv[3])),
  async archive => {
    await copyFile(archive, output, constants.COPYFILE_EXCL)
    const file = await open(output, 'r')
    try {
      await file.sync()
    } finally {
      await file.close()
    }
  },
)
