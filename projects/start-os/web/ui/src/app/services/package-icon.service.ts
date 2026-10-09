import { inject, Service } from '@angular/core'
import { FALLBACK_ICON, registryIconUrl } from '@start9labs/marketplace'
import { PatchDB } from 'patch-db-client'
import { catchError, from, Observable, of, shareReplay, switchMap } from 'rxjs'
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
      return of(
        pkg.registry
          ? registryIconUrl(pkg.registry, id, version, dependency)
          : FALLBACK_ICON,
      )
    }

    const path = dependency ? `dependencies/${dependency}/icon` : 'icon'
    const key = `${pkg.s9pk}/${path}`
    const cached = this.cache.get(key)

    if (cached) {
      return cached
    }

    const icon$ = from(
      this.api.getStaticObjectUrl(`/s9pk/installed/${id}.s9pk/${path}`),
    ).pipe(
      catchError(() => {
        this.cache.delete(key)
        return of(FALLBACK_ICON)
      }),
      shareReplay(1),
    )
    this.cache.set(key, icon$)

    return icon$
  }
}
