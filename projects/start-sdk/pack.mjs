#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { rmSync } from 'node:fs'
import { requiresSharedVolumes } from './check-cli.mjs'

const [output, ...args] = process.argv.slice(2)
try {
  execFileSync('start-cli', ['s9pk', 'pack', ...args, '-o', output], {
    stdio: 'inherit',
  })
  if (requiresSharedVolumes) {
    const manifest = JSON.parse(
      execFileSync(
        'start-cli',
        ['s9pk', 'inspect', output, 'cat', 'manifest.json'],
        {
          encoding: 'utf8',
        },
      ),
    )
    if (manifest.sharedVolumes !== true)
      throw new Error(
        'The packer did not preserve sharedVolumes. Upgrade start-cli to a volume-capable build.',
      )
  }
} catch (error) {
  rmSync(output, { force: true })
  throw error
}
