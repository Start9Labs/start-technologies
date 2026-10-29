#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'
import { Version } from '@start9labs/start-core/exver'

const { manifest } = createRequire(import.meta.url)(
  resolve('javascript/index.js'),
)
if (manifest.sharedVolumes) {
  const required = '2.6.0'
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
