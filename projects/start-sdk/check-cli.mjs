#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { realpathSync } from 'node:fs'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { Version } from '@start9labs/start-core/exver'

const { manifest } = createRequire(import.meta.url)(
  resolve('javascript/index.js'),
)
export const requiresSharedVolumes = !!manifest.sharedVolumes

function checkCli() {
  if (!requiresSharedVolumes) return
  const required = '2.8.0'
  const installed = execFileSync('start-cli', ['--version'], {
    encoding: 'utf8',
  })
    .trim()
    .replace(/^start-cli\s+/, '')
  if (Version.parse(installed).compare(Version.parse(required)) === 'less') {
    throw new Error(
      `Volume packages require start-cli ${required} or later; found ${installed}`,
    )
  }
}

if (
  process.argv[1] &&
  realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)
)
  checkCli()
