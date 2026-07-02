// @ts-check
const fs = require('fs')
const path = require('path')
const childProcess = require('child_process')

// Resolve paths relative to this script so it works from any cwd (the root
// workspace runs it as `node projects/start-wrt/web/build-config.js`).
const configPath = path.join(__dirname, 'config.json')

const gitHash = String(
  childProcess.execSync('git describe --always --abbrev=40 --dirty=-modified'),
).trim()

const origConfig = require(configPath)

origConfig['gitHash'] = gitHash
fs.writeFileSync(configPath, JSON.stringify(origConfig, null, 2))
