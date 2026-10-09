import { inject, Service } from '@angular/core'
import { FALLBACK_ICON, registryIconUrl } from '@start9labs/marketplace'
import { PatchDB } from 'patch-db-client'
import { Observable, of, switchMap } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import {
  DataModel,
  PackageDataEntry,
} from 'src/app/services/patch-db/data-model'
import {
  getManifest,
  isInstalling,
  isRestoring,
} from 'src/app/utils/get-package-data'

import { IconObjectUrlCache } from './icon-object-url-cache'

@Service()
export class PackageIconService {
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly cache = new IconObjectUrlCache(
    url => this.api.getStaticObjectUrl(url),
    FALLBACK_ICON,
  )

  getRegistry$(
    registry: string,
    id: string,
    version: string,
    dependency?: string,
  ): Observable<string> {
    const url = `/registry/icons/${encodeURIComponent(
      registryIconUrl(registry, id, version, dependency),
    )}`
    return this.cache.get(url, url)
  }

  get(pkg: PackageDataEntry | string, dependency?: string): Observable<string> {
    if (typeof pkg === 'string') {
      return this.patch
        .watch$('packageData', pkg)
        .pipe(
          switchMap(entry =>
            entry ? this.get(entry, dependency) : of(FALLBACK_ICON),
          ),
        )
    }

    const { id, version } = getManifest(pkg)

    if (isInstalling(pkg) || isRestoring(pkg)) {
      return pkg.registry
        ? this.getRegistry$(pkg.registry, id, version, dependency)
        : of(FALLBACK_ICON)
    }

    const path = dependency ? `dependencies/${dependency}/icon` : 'icon'
    return this.cache.get(
      `${pkg.s9pk}/${path}`,
      `/s9pk/installed/${id}.s9pk/${path}`,
    )
  }
}
