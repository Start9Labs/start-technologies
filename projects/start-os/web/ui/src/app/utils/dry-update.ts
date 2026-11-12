import type { ApiService } from '../services/api/embassy-api.service'
import type { Params } from '../services/api/api.types'
import type { DataModel } from '../services/patch-db/data-model'
import { getManifest } from './get-package-data'

export async function dryUpdate(
  params: Params<'package.preview-provider-change'>,
  packages: DataModel['packageData'],
  api: ApiService,
): Promise<string[]> {
  const affected = await api.rpc.package.previewProviderChange(params)
  return affected.map(id => {
    const pkg = packages[id]
    return pkg ? getManifest(pkg).title : id
  })
}
