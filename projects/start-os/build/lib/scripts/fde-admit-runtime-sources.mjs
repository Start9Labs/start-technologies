#!/usr/bin/env node
import { resolve } from 'node:path'

import { admitRuntimeSources } from './fde-harden-initramfs.mjs'

if (process.argv.length !== 3)
  throw Error('Usage: fde-admit-runtime-sources.mjs <private-config-directory>')
await admitRuntimeSources('/', resolve(process.argv[2]))
