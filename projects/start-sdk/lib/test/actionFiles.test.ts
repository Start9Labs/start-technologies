import { setupManifest } from '../manifest/setupManifest'
import { StartSdk } from '../StartSdk'
import { Effects } from '@start9labs/start-core/types'

const sdk = StartSdk.of()
  .withManifest(
    setupManifest({
      id: 'file-service',
      title: 'Files',
      license: 'MIT',
      packageRepo: '',
      upstreamRepo: '',
      marketingUrl: '',
      donationUrl: null,
      description: { short: '', long: '' },
      images: {},
      volumes: ['data' as const],
    }),
  )
  .build(true)

test('file helpers forward declared volume destinations and optional attributes', async () => {
  const prepareUpload = jest.fn(async () => 'upload-id')
  const prepareDownload = jest.fn(async () => 'download-id')
  const effects = { prepareUpload, prepareDownload } as unknown as Effects
  const upload = {
    volume: 'data' as const,
    path: 'input.pem',
    uid: 1000,
    gid: 1000,
    mode: 0o600,
    maxSize: 1024,
  }
  const download = {
    volume: 'data' as const,
    path: 'report.zip',
    mime: 'application/zip',
    filename: 'report.zip',
  }
  await expect(sdk.prepareUpload(effects, upload)).resolves.toBe('upload-id')
  await expect(sdk.prepareDownload(effects, download)).resolves.toBe(
    'download-id',
  )
  expect(prepareUpload).toHaveBeenCalledWith(upload)
  expect(prepareDownload).toHaveBeenCalledWith(download)
})

function checkFileOptions(effects: Effects) {
  // @ts-expect-error Volume IDs come from the bound manifest.
  sdk.prepareUpload(effects, { volume: 'unknown', path: 'input' })
  // @ts-expect-error Volume IDs come from the bound manifest.
  sdk.prepareDownload(effects, { volume: 'unknown', path: 'output' })
  sdk.prepareUpload(effects, {
    volume: 'data',
    path: 'input',
    // @ts-expect-error Extension filtering belongs to the File input.
    extensions: ['.pem'],
  })
  sdk.prepareUpload(effects, {
    volume: 'data',
    path: 'input',
    // @ts-expect-error Upload preparation has no minimum-size option.
    minSize: 1,
  })
}
