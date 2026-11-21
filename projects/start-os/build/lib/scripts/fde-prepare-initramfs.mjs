#!/usr/bin/env node
import { readFile } from 'node:fs/promises'
import { resolve } from 'node:path'

import {
  installBootDescriptor,
  prepareGeneratedInitramfs,
} from './fde-initramfs-preparation.mjs'

if (
  process.argv.length !== 3 ||
  !process.env.FDE_PROVIDER_ALIASES ||
  !process.env.FDE_BOOT_DESCRIPTOR
)
  throw Error('Usage: fde-prepare-initramfs.mjs <private-generated-tree>')
await prepareGeneratedInitramfs(
  resolve(process.argv[2]),
  JSON.parse(await readFile(process.env.FDE_PROVIDER_ALIASES, 'utf8')),
)
await installBootDescriptor(
  resolve(process.argv[2]),
  await readFile(process.env.FDE_BOOT_DESCRIPTOR),
)
