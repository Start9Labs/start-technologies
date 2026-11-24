import { Effects } from '@start9labs/start-core/types'
import { StartSdk } from '../StartSdk'
import { setupManifest } from '../manifest/setupManifest'

const manifest = setupManifest({
  id: 'proxy',
  title: 'Proxy',
  license: 'MIT',
  packageRepo: '',
  upstreamRepo: '',
  marketingUrl: '',
  donationUrl: null,
  description: { short: '', long: '' },
  images: {},
  volumes: [],
  plugins: ['http-proxy-v0'],
})

test('HTTP proxy helpers require the manifest capability', () => {
  const sdk = StartSdk.of()
    .withManifest({ ...manifest, plugins: [] })
    .build(true)
  expect(sdk.plugin.httpProxy).toBeNull()
})

test('HTTP proxy helpers delegate profiles and expose a standard reactive reader', async () => {
  const sdk = StartSdk.of().withManifest(manifest).build(true)
  const setProfiles = jest.fn(async () => null)
  const getAttachments = jest.fn(async () => [])
  const effects = {
    plugin: { httpProxy: { setProfiles, getAttachments } },
  } as unknown as Effects
  await sdk.plugin.httpProxy.setProfiles(effects, [
    { id: 'standard', name: 'Standard', protocols: ['http1'] },
  ])
  expect(setProfiles).toHaveBeenCalledWith({
    profiles: [{ id: 'standard', name: 'Standard', protocols: ['http1'] }],
  })
  const reader = sdk.plugin.httpProxy.getAttachments(effects)
  expect(await reader.once()).toEqual([])
  expect(typeof reader.const).toBe('function')
  expect(typeof reader.watch).toBe('function')
})
