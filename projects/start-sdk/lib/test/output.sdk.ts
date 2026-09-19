import { StartSdk } from '../StartSdk'
import { setupManifest } from '../manifest/setupManifest'
import { VersionGraph } from '../version/VersionGraph'

export type Manifest = any
export const sdk = StartSdk.of()
  .withManifest(
    setupManifest({
      id: 'testOutput',
      title: '',
      license: '',
      packageRepo: '',
      upstreamRepo: '',
      marketingUrl: '',
      donationUrl: null,
      description: {
        short: '',
        long: '',
      },
      images: {
        main: {
          source: {
            dockerTag: 'start9/hello-world',
          },
          arch: ['aarch64', 'x86_64'],
          emulateMissing: true,
        },
      },
      volumes: [],
      dependencies: {
        'remote-test': {
          description: '',
          optional: false,
          s9pk: 'https://example.com/remote-test.s9pk',
        },
      },
    }),
  )
  .build(true)
