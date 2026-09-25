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
