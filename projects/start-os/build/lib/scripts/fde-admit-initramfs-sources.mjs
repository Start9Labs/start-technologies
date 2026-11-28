#!/usr/bin/env node
import { resolve } from 'node:path'

import { admitInitramfsSources } from './fde-initramfs-control.mjs'

if (process.argv.length !== 3)
  throw Error(
    'Usage: fde-admit-initramfs-sources.mjs <private-config-directory>',
  )
await admitInitramfsSources('/', resolve(process.argv[2]))
