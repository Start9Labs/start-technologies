#!/usr/bin/env node
// Invoked by s9pk.mk from the package directory; the glob below is relative to that cwd.
import { ESLint } from 'eslint'
import config from './eslint.config.base.mjs'

const eslint = new ESLint({
  cwd: process.cwd(),
  overrideConfigFile: true,
  overrideConfig: config,
  errorOnUnmatchedPattern: false,
})

const results = await eslint.lintFiles(['startos/**/*.ts'])
const formatter = await eslint.loadFormatter('stylish')
const output = await formatter.format(results)
if (output.trim()) console.error(output)

const errors = results.reduce((n, r) => n + r.errorCount, 0)
process.exit(errors > 0 ? 1 : 0)
