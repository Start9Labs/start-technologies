import { buildManifest, setupManifest } from '../manifest/setupManifest'
import { Dependencies, Dependency } from '../dependencies'
import { VersionGraph } from '../version/VersionGraph'
import { VersionInfo } from '../version/VersionInfo'

const versions = VersionGraph.of({
  current: VersionInfo.of<'1.0.0:0'>({
    version: '1.0.0:0',
    releaseNotes: '',
    migrations: {},
  }),
  other: [],
})

function manifest(emulateMissing?: boolean) {
  return setupManifest({
    id: 'test',
    title: 'Test',
    license: 'MIT',
    packageRepo: 'https://example.com/package',
    upstreamRepo: 'https://example.com/upstream',
    marketingUrl: '',
    donationUrl: null,
    preDownloadAlert: {
      message: {
        en_US: 'Back up before updating',
        fr_FR: 'Sauvegardez avant la mise à jour',
      },
      when: { sourceVersion: '<1.0.0:0' },
    },
    description: { short: 'Test', long: 'Test' },
    images: {
      main: {
        source: { dockerTag: 'example/test:1.0.0' },
        arch: ['aarch64'],
        ...(emulateMissing === undefined ? {} : { emulateMissing }),
      },
    },
    volumes: [],
  })
}

test('defaults to the action file transfer compatibility floor', () => {
  expect(buildManifest(versions, manifest(), Dependencies.of()).osVersion).toBe(
    '0.4.2',
  )
})

test('pre-download alert is included in the built manifest', () => {
  expect(
    buildManifest(versions, manifest(), Dependencies.of()).preDownloadAlert,
  ).toEqual({
    message: {
      en_US: 'Back up before updating',
      fr_FR: 'Sauvegardez avant la mise à jour',
    },
    when: { sourceVersion: '<1.0.0:0' },
  })
})

test('the builder publishes its complete base dependency requirements', () => {
  const dependencies = Dependencies.of().addDependency(
    Dependency.required('bitcoind', {
      description: 'Blockchain data',
      metadata: { title: 'Bitcoin', icon: 'https://example.com/icon.png' },
      versionRange: '>=31.1:17',
      kind: 'running',
      healthChecks: ['bitcoin-rest'],
    }),
  )
  const built = buildManifest(versions, manifest(), dependencies)
  expect(built.dependencies.bitcoind).toEqual({
    description: 'Blockchain data',
    optional: false,
    versionRange: '>=31.1:17',
    kind: 'running',
    healthChecks: ['bitcoin-rest'],
    metadata: { title: 'Bitcoin', icon: 'https://example.com/icon.png' },
  })
})

test('compatibility metadata defaults to empty claims', () => {
  const built = buildManifest(versions, manifest(), Dependencies.of())

  expect(built.satisfies).toEqual([])
  expect(built.satisfiesPackages).toEqual({})
})

test('the manifest publishes current compatibility claims, not historical claims', () => {
  const graph = VersionGraph.of({
    current: versions.current
      .satisfies('#fork:1.0.0:0')
      .satisfies('bitcoind', '28.0:0')
      .satisfies('bitcoind', '29.0:0'),
    other: [
      VersionInfo.of({
        version: '0.9.0:0',
        releaseNotes: '',
        migrations: {},
      }).satisfies('historical-provider', '1.0.0:0'),
    ],
  })
  const built = buildManifest(graph, manifest(), Dependencies.of())

  expect(built.satisfies).toEqual(['#fork:1.0.0:0'])
  expect(built.satisfiesPackages).toEqual({ bitcoind: ['28.0:0', '29.0:0'] })
  expect(built.osVersion).toBe('0.4.3')
  expect(
    buildManifest(
      graph,
      { ...manifest(), osVersion: '0.4.0' },
      Dependencies.of(),
    ).osVersion,
  ).toBe('0.4.3')
  expect(
    buildManifest(
      graph,
      { ...manifest(), osVersion: '0.5.0' },
      Dependencies.of(),
    ).osVersion,
  ).toBe('0.5.0')
})

test('package-qualified claims reject the manifest own ID', () => {
  const graph = VersionGraph.of({
    current: versions.current.satisfies('test', '2.0.0:0'),
    other: [],
  })
  expect(() => buildManifest(graph, manifest(), Dependencies.of())).toThrow(
    'Use .satisfies(version)',
  )
  const samePackage = VersionGraph.of({
    current: versions.current.satisfies('2.0.0:0'),
    other: [],
  })
  expect(
    buildManifest(samePackage, manifest(), Dependencies.of()).satisfies,
  ).toEqual(['2.0.0:0'])
})

test('images emulate missing architectures by default', () => {
  const built = buildManifest(versions, manifest(), Dependencies.of())

  expect(built.images.main.emulateMissing).toBe(true)
  expect(built.hardwareRequirements.arch).toBeNull()
})

test('images can require a native architecture', () => {
  const built = buildManifest(versions, manifest(false), Dependencies.of())

  expect(built.images.main.emulateMissing).toBe(false)
  expect(built.hardwareRequirements.arch).toEqual(['aarch64'])
})
