import { VersionInfo } from '../version/VersionInfo'

function version() {
  return VersionInfo.of({
    version: '5.0.0:0',
    releaseNotes: '',
    migrations: {},
  })
}

test('compatibility claims start empty without claiming the package version', () => {
  const info = version()

  expect(info.options.satisfies).toEqual([])
  expect(info.options.satisfiesPackages).toEqual({})
})

test('own-package and cross-package claims accumulate independently', () => {
  const info = version()
    .satisfies('4.0.0:0')
    .satisfies('bitcoind', '28.0:0')
    .satisfies('other-package', '#fork:3.0.0:1')
    .satisfies('bitcoind', '29.0:0')
    .satisfies('#fork:4.0.0:0')

  expect(info.options.satisfies).toEqual(['4.0.0:0', '#fork:4.0.0:0'])
  expect(info.options.satisfiesPackages).toEqual({
    bitcoind: ['28.0:0', '29.0:0'],
    'other-package': ['#fork:3.0.0:1'],
  })
})

test('adding claims leaves previous builders unchanged', () => {
  const initial = version()
  const own = initial.satisfies('4.0.0:0')
  const provider = own.satisfies('bitcoind', '28.0:0')
  const extended = provider.satisfies('bitcoind', '29.0:0')
  const sibling = provider.satisfies('other-package', '1.0.0:0')

  expect(initial.options.satisfies).toEqual([])
  expect(own.options.satisfiesPackages).toEqual({})
  expect(provider.options.satisfiesPackages).toEqual({ bitcoind: ['28.0:0'] })
  expect(extended.options.satisfiesPackages).toEqual({
    bitcoind: ['28.0:0', '29.0:0'],
  })
  expect(sibling.options.satisfiesPackages).toEqual({
    bitcoind: ['28.0:0'],
    'other-package': ['1.0.0:0'],
  })
})

test('package IDs that name object properties can declare compatibility', () => {
  const info = version().satisfies('constructor', '1.0.0:0')

  expect(info.options.satisfiesPackages).toEqual({ constructor: ['1.0.0:0'] })
})
