import * as fs from 'fs'
import * as path from 'path'

const {
  assertPackageSubpaths,
} = require('../../../scripts/assert-package-subpaths.cjs')

const packageRoot = path.resolve(__dirname, '..', '..')
const lib = path.join(packageRoot, 'lib')
const entries = JSON.parse(
  fs.readFileSync(path.join(packageRoot, 'package.json'), 'utf8'),
).exports

const indexDirectories = (dir: string): string[] =>
  fs
    .readdirSync(dir, { withFileTypes: true })
    .filter(entry => entry.isDirectory() && entry.name !== 'test')
    .flatMap(entry => {
      const child = path.join(dir, entry.name)
      const rel = path.relative(lib, child)
      const own = fs.existsSync(path.join(child, 'index.ts')) ? [rel] : []
      return [...own, ...indexDirectories(child)]
    })

test.each([
  ['util/once', ['/util/once', '/util/once.js']],
  ['util/index', ['/util', '/util/index', '/util/index.js']],
  ['index', ['', '/index', '/index.js']],
])(
  '%s resolves in CommonJS and ESM with and without .js',
  (modulePath, subpaths) => {
    assertPackageSubpaths(packageRoot, modulePath, subpaths)
  },
)

test('every directory a consumer can import by its index has an exports entry', () => {
  for (const dir of indexDirectories(lib)) {
    expect(entries[`./${dir}`]).toEqual({
      types: `./${dir}/index.d.ts`,
      import: `./esm/${dir}/index.mjs`,
      default: `./${dir}/index.js`,
    })
  }
})
