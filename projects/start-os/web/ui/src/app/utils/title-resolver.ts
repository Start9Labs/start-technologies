import { inject } from '@angular/core'
import { ActivatedRouteSnapshot } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom } from 'rxjs'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'

export async function titleResolver({
  data,
  params,
}: ActivatedRouteSnapshot): Promise<string> {
  let route = inject(i18nPipe).transform(data['title'])

  const patch = inject<PatchDB<DataModel>>(PatchDB)
  const title = await firstValueFrom(patch.watch$('serverInfo', 'hostname'))
  const id = params['pkgId']

  if (id) {
    const service = await firstValueFrom(patch.watch$('packageData', id))

    route = service && getManifest(service).title
  }

  return `${title || 'StartOS'} — ${route}`
}
