import * as fs from 'fs'
import * as path from 'path'

const {
  assertPackageSubpaths,
} = require('../../../../shared-libs/ts-modules/scripts/assert-package-subpaths.cjs')

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
  [
    'lib/trigger/defaultTrigger',
    ['/lib/trigger/defaultTrigger', '/lib/trigger/defaultTrigger.js'],
  ],
  [
    'lib/trigger/index',
    ['/lib/trigger', '/lib/trigger/index', '/lib/trigger/index.js'],
  ],
  ['lib/index', ['', '/lib', '/lib/index', '/lib/index.js']],
])(
  '%s resolves in CommonJS and ESM with and without .js',
  (modulePath, subpaths) => {
    assertPackageSubpaths(packageRoot, modulePath, subpaths)
  },
)

test('every directory a consumer can import by its index has an exports entry', () => {
  for (const dir of indexDirectories(lib)) {
    expect(entries[`./lib/${dir}`]).toEqual({
      types: `./lib/${dir}/index.d.ts`,
      import: `./esm/lib/${dir}/index.mjs`,
      default: `./lib/${dir}/index.js`,
    })
  }
})
