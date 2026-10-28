import { inject, Service } from '@angular/core'
import { FALLBACK_ICON, registryIconUrl } from '@start9labs/marketplace'
import { PatchDB } from 'patch-db-client'
import { catchError, defer, Observable, of, shareReplay, switchMap } from 'rxjs'
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

@Service()
export class PackageIconService {
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly cache = new Map<string, Observable<string>>()

  getRegistry$(registry: string, icon: string): Observable<string> {
    const url = `/registry/icons/${encodeURIComponent(
      registryIconUrl(registry, icon),
    )}`
    return this.load(url, url)
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

    const { id } = getManifest(pkg)
    const path = dependency ? `dependencies/${dependency}/icon` : 'icon'
    const source =
      isInstalling(pkg) || isRestoring(pkg) ? 'installing' : 'installed'

    const url = `/s9pk/${source}/${id}.s9pk/${path}`

    return this.load(
      `${source}:${pkg.s9pk}/${path}`,
      source === 'installed'
        ? `${url}?v=${encodeURIComponent(pkg.s9pk.split('/').pop()!)}`
        : url,
    )
  }

  private load(key: string, url: string): Observable<string> {
    const cached = this.cache.get(key)

    if (cached) {
      return cached
    }

    const icon$ = defer(() => this.api.getStaticObjectUrl(url)).pipe(
      catchError(() => of(FALLBACK_ICON)),
      shareReplay(1),
    )
    this.cache.set(key, icon$)

    return icon$
  }
}
