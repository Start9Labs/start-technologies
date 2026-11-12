#!/usr/bin/env node
import { resolve } from 'node:path'

import { harden } from './fde-initramfs-control.mjs'

if (process.argv.length !== 3)
  throw Error('Usage: fde-harden-initramfs.mjs <private-generated-tree>')
await harden(resolve(process.argv[2]))
