const { execFileSync } = require('node:child_process')
const {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} = require('node:fs')
const { tmpdir } = require('node:os')
const { dirname, join } = require('node:path')

exports.assertPackageSubpaths = (packageRoot, modulePath, subpaths) => {
  const metadata = JSON.parse(
    readFileSync(join(packageRoot, 'package.json'), 'utf8'),
  )
  const fixture = mkdtempSync(join(tmpdir(), 'package-subpaths-'))
  try {
    const root = join(fixture, 'node_modules', metadata.name)
    const cjs = join(root, `${modulePath}.js`)
    const esm = join(root, 'esm', `${modulePath}.mjs`)
    mkdirSync(dirname(cjs), { recursive: true })
    mkdirSync(dirname(esm), { recursive: true })
    writeFileSync(join(root, 'package.json'), JSON.stringify(metadata))
    writeFileSync(cjs, "exports.format = 'cjs'\n")
    writeFileSync(esm, "export const format = 'esm'\n")
    const specifiers = JSON.stringify(
      subpaths.map(subpath => metadata.name + subpath),
    )
    execFileSync(
      process.execPath,
      [
        '-e',
        `
      const assert = require('node:assert/strict')
      for (const specifier of JSON.parse(process.argv[1])) {
        assert.equal(require(specifier).format, 'cjs', specifier)
      }
    `,
        specifiers,
      ],
      { cwd: fixture, stdio: 'pipe' },
    )
    execFileSync(
      process.execPath,
      [
        '--input-type=module',
        '-e',
        `
      import assert from 'node:assert/strict'
      for (const specifier of JSON.parse(process.argv[1])) {
        assert.equal((await import(specifier)).format, 'esm', specifier)
      }
    `,
        specifiers,
      ],
      { cwd: fixture, stdio: 'pipe' },
    )
  } finally {
    rmSync(fixture, { recursive: true, force: true })
  }
}
