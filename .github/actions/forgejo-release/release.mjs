import { openAsBlob } from 'node:fs'
import { basename } from 'node:path'

export async function publishRelease({ request, tag, name, body, files }) {
  const metadata = { tag_name: tag, name, body }
  let release = await request(
    'GET',
    `/releases/tags/${encodeURIComponent(tag)}`,
    { allowNotFound: true },
  )
  release = release
    ? await request('PATCH', `/releases/${release.id}`, { json: metadata })
    : await request('POST', '/releases', { json: metadata })

  for (const file of files) {
    const assetName = basename(file)
    const existing = release.assets?.find(asset => asset.name === assetName)
    if (existing)
      await request('DELETE', `/releases/${release.id}/assets/${existing.id}`)
    const form = new FormData()
    form.set('attachment', await openAsBlob(file), assetName)
    await request(
      'POST',
      `/releases/${release.id}/assets?name=${encodeURIComponent(assetName)}`,
      { body: form },
    )
  }
}
