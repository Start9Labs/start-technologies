import { buildManifest, setupManifest } from '../manifest/setupManifest'
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
      message: 'Back up before updating',
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
    dependencies: {},
  })
}

test('pre-download alert is included in the built manifest', () => {
  expect(buildManifest(versions, manifest()).preDownloadAlert).toEqual({
    message: 'Back up before updating',
    when: { sourceVersion: '<1.0.0:0' },
  })
})

test('images emulate missing architectures by default', () => {
  const built = buildManifest(versions, manifest())

  expect(built.images.main.emulateMissing).toBe(true)
  expect(built.hardwareRequirements.arch).toBeNull()
})

test('images can require a native architecture', () => {
  const built = buildManifest(versions, manifest(false))

  expect(built.images.main.emulateMissing).toBe(false)
  expect(built.hardwareRequirements.arch).toEqual(['aarch64'])
})
