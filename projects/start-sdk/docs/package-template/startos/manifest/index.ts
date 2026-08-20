import { setupManifest } from '@start9labs/start-sdk'
import { long, short } from './i18n'

export const manifest = setupManifest({
  id: '{{id}}',
  title: '{{name}}',
  license: 'MIT', // TODO: match the upstream project's license
  packageRepo: 'https://github.com/REPLACE_ME/{{id}}-startos', // TODO: set the packaging repo URL
  upstreamRepo: 'https://github.com/REPLACE_ME/REPLACE_ME', // TODO: set the upstream project URL
  marketingUrl: 'https://REPLACE_ME', // TODO: set or remove
  donationUrl: 'https://REPLACE_ME', // TODO: set or remove
  description: { short, long },
  // 'example-volume' is an arbitrary id — name volumes whatever suits the
  // service. It must match the volumeId mounted in startos/main.ts and the
  // volume backed up in startos/backups.ts.
  volumes: ['example-volume'],
  images: {
    // 'example-image' is an arbitrary id — it must match the imageId used in
    // startos/main.ts. TODO: replace the hello-world image with your service's
    // image — set dockerTag (or add a Dockerfile) and rename this key.
    'example-image': {
      source: { dockerTag: 'ghcr.io/start9labs/hello-world:2.0.0' },
      arch: ['x86_64', 'aarch64'],
    },
  },
  dependencies: {},
})
