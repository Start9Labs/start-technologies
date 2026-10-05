import { inject, Injectable } from '@angular/core'
import {
  AbstractMarketplaceService,
  GetPackageRes,
  Marketplace,
  MarketplacePkg,
  StoreDataWithUrl,
  StoreIdentity,
} from '@start9labs/marketplace'
import {
  defaultRegistries,
  Exver,
  i18nPipe,
  registryUrl,
  sameUrl,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  BehaviorSubject,
  catchError,
  combineLatest,
  distinctUntilChanged,
  filter,
  firstValueFrom,
  from,
  map,
  mergeMap,
  Observable,
  of,
  pairwise,
  ReplaySubject,
  scan,
  shareReplay,
  startWith,
  switchMap,
  tap,
} from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { StorageService } from 'src/app/services/storage.service'

const { start9, community } = defaultRegistries

@Injectable({
  providedIn: 'root',
})
export class MarketplaceService extends AbstractMarketplaceService {
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly exver = inject(Exver)
  private readonly storage = inject(StorageService)
  private readonly i18n = inject(i18nPipe)

  readonly registries$: Observable<StoreIdentity[]> = this.patch
    .watch$('ui', 'registries')
    .pipe(
      map(registries => [
        toStoreIdentity(start9, registries[start9]),
        toStoreIdentity(community, registries[community]),
        ...Object.entries(registries)
          .filter(([u, _]) => !sameUrl(start9, u) && !sameUrl(community, u))
          .map(([url, name]) => toStoreIdentity(url, name)),
      ]),
    )

  readonly newRegistry$ = this.patch.watch$('ui', 'registries').pipe(
    map(registries =>
      [start9, community, ...Object.keys(registries)]
        .filter((url, i, all) => all.findIndex(u => sameUrl(u, url)) === i)
        .map(url => ({ url, name: registries[url] ?? null })),
    ),
    startWith<{ url: string; name: string | null }[]>([]),
    pairwise(),
    mergeMap(([p, c]) => c.filter(a => !p.find(b => sameUrl(a.url, b.url)))),
  )

  readonly currentRegistryUrl$ = new ReplaySubject<string>(1)

  readonly requestErrors$ = new BehaviorSubject<string[]>([])

  readonly marketplace$: Observable<Marketplace> = combineLatest([
    this.newRegistry$.pipe(
      mergeMap(({ url, name }) =>
        this.fetchRegistry$(url).pipe(
          tap(data => {
            if (data?.info.name)
              this.updateRegistryName(url, name, data.info.name)
          }),
          map(data => [url, data] satisfies [string, StoreDataWithUrl | null]),
          startWith<[string, StoreDataWithUrl | null]>([url, null]),
        ),
      ),
      scan<[string, StoreDataWithUrl | null], Marketplace>(
        (requests, [url, store]) => ({
          ...requests,
          [url]: store,
        }),
        {},
      ),
    ),
    this.registries$,
  ]).pipe(
    map(([marketplace, registries]) =>
      Object.fromEntries(
        Object.entries(marketplace).filter(([url]) =>
          registries.find(store => sameUrl(store.url, url)),
        ),
      ),
    ),
    shareReplay(1),
  )

  readonly currentRegistry$: Observable<StoreDataWithUrl> = combineLatest([
    this.marketplace$,
    this.currentRegistryUrl$,
    this.currentRegistryUrl$.pipe(
      distinctUntilChanged(),
      switchMap(url => this.fetchRegistry$(url).pipe(startWith(null))),
    ),
  ]).pipe(
    map(([all, url, current]) => current || all[url]),
    filter(Boolean),
    shareReplay(1),
  )

  readonly registryIcons$ = combineLatest([
    this.currentRegistry$,
    this.marketplace$,
  ]).pipe(
    map(([current, marketplace]) => [
      { url: current.url, icon: current.info.icon },
      ...Object.values(marketplace).flatMap(registry =>
        registry ? [{ url: registry.url, icon: registry.info.icon }] : [],
      ),
    ]),
  )

  getPackage$(
    id: string,
    version: string | null,
    flavor: string | null,
    registryUrl?: string,
  ): Observable<MarketplacePkg | null> {
    return this.currentRegistry$.pipe(
      switchMap(registry => {
        const url = registryUrl || registry.url
        const pkg = registry.packages.find(
          p =>
            p.id === id &&
            p.flavor === flavor &&
            (!version || this.exver.compareExver(p.version, version) === 0),
        )
        return pkg ? of(pkg) : this.fetchPackage$(url, id, version, flavor)
      }),
    )
  }

  fetchInfo$(registry: string): Observable<T.RegistryInfo> {
    return from(this.api.getRegistryInfo({ registry })).pipe(
      map(info => ({
        ...info,
        categories: {
          all: { name: 'All' },
          ...info.categories,
        },
      })),
    )
  }

  fetchStatic$(
    pkg: MarketplacePkg,
    path: 'LICENSE.md' | 'instructions.md',
  ): Observable<string> {
    const registryAsset = pkg.s9pks[0]?.[1]

    if (!registryAsset) {
      throw new Error('No s9pk')
    }

    const urls =
      registryAsset.urls.map(
        u => `/s9pk/proxy/${encodeURIComponent(u)}/${path}`,
      ) || []

    return from(
      this.api.getStatic(urls, {
        rootSighash: registryAsset.commitment.rootSighash,
        rootMaxsize: registryAsset.commitment.rootMaxsize,
      }),
    )
  }

  private fetchRegistry$(url: string): Observable<StoreDataWithUrl | null> {
    return combineLatest([this.fetchInfo$(url), this.fetchPackages$(url)]).pipe(
      map(([info, packages]) => ({ info, packages, url })),
      catchError(e => {
        console.error(e)
        this.requestErrors$.next(this.requestErrors$.value.concat(url))
        return of(null)
      }),
    )
  }

  private fetchPackages$(url: string): Observable<MarketplacePkg[]> {
    return from(
      this.api.getRegistryPackages({
        registry: url,
        id: null,
        targetVersion: null,
        sourceVersion: null,
        otherVersions: 'short',
      }),
    ).pipe(
      map(packages =>
        Object.entries(packages).flatMap(([id, pkgInfo]) =>
          Object.keys(pkgInfo.best).flatMap(version => {
            const pkg = this.convertRegistryPkgToMarketplacePkg(
              id,
              version,
              this.exver.getFlavor(version),
              pkgInfo,
            )
            return pkg ? [pkg] : []
          }),
        ),
      ),
    )
  }

  fetchPackage$(
    url: string,
    id: string,
    version: string | null,
    flavor: string | null,
    sourceVersion: string | null = null,
  ): Observable<MarketplacePkg | null> {
    return from(
      this.api.getRegistryPackage({
        registry: url,
        id,
        targetVersion: version ? `=${version}` : null,
        sourceVersion,
        otherVersions: 'short',
      }),
    ).pipe(
      map(pkgInfo =>
        this.convertRegistryPkgToMarketplacePkg(id, version, flavor, pkgInfo),
      ),
    )
  }

  private convertRegistryPkgToMarketplacePkg(
    id: string,
    version: string | null | undefined,
    flavor: string | null,
    pkgInfo: GetPackageRes,
  ): MarketplacePkg | null {
    const ver =
      version ||
      Object.keys(pkgInfo.best).find(v => this.exver.getFlavor(v) === flavor) ||
      null
    const best = ver && pkgInfo.best[ver]

    if (!best) {
      return null
    }

    return {
      id,
      flavor,
      version: ver || '',
      ...pkgInfo,
      ...best,
    }
  }

  async installPackage(
    id: string,
    version: string,
    url: string,
  ): Promise<void> {
    const params: T.InstallParams = {
      id,
      version,
      registry: url,
    }

    await this.api.installPackage(params)
  }

  async connect(url: string): Promise<void> {
    this.currentRegistryUrl$.next(url)
    this.storage.set('selectedRegistry', url)
  }

  async add(rawUrl: string): Promise<string> {
    const url = registryUrl(rawUrl)
    const existing = await firstValueFrom(this.registries$)

    if (existing.some(r => sameUrl(r.url, url))) {
      throw new Error(this.i18n.transform('Registry already added'))
    }

    // validates the registry is reachable and provides a display name
    const info = await firstValueFrom(this.fetchInfo$(url))
    await this.api.setDbValue<string | null>(['registries', url], info.name)

    return url
  }

  async delete(url: string): Promise<void> {
    const raw = await firstValueFrom(this.patch.watch$('ui', 'registries'))
    const filtered: Record<string, string | null> = Object.fromEntries(
      Object.entries(raw).filter(([key]) => !sameUrl(key, url)),
    )
    await this.api.setDbValue(['registries'], filtered)

    // If the deleted registry was the persisted selection, clear it so the next
    // load falls back to the default rather than the deleted one.
    const selected = this.storage.get<string>('selectedRegistry')
    if (selected && sameUrl(selected, url)) {
      this.storage.set('selectedRegistry', null)
    }
  }

  private async updateRegistryName(
    url: string,
    oldName: string | null,
    newName: string,
  ): Promise<void> {
    if (oldName !== newName) {
      this.api.setDbValue<string>(['registries', url], newName)
    }
  }
}

function toStoreIdentity(url: string, name?: string | null): StoreIdentity {
  return {
    url,
    name: name || url,
  }
}
