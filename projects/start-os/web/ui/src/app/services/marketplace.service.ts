import { inject, Injectable } from '@angular/core'
import {
  AbstractMarketplaceService,
  GetPackageRes,
  Marketplace,
  MarketplacePkg,
  StoreDataWithUrl,
  StoreIdentity,
  packageResponse,
  packagesResponse,
  packageSignerPolicy,
  satisfiesWithAliases,
} from '@start9labs/marketplace'
import { Exver, registryUrl, sameUrl } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  BehaviorSubject,
  catchError,
  combineLatest,
  distinctUntilChanged,
  filter,
  from,
  firstValueFrom,
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
} from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { PackageIconService } from 'src/app/services/package-icon.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { StorageService } from 'src/app/services/storage.service'

@Injectable({
  providedIn: 'root',
})
export class MarketplaceService extends AbstractMarketplaceService {
  private readonly api = inject(ApiService)
  private readonly icons = inject(PackageIconService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly exver = inject(Exver)
  private readonly storage = inject(StorageService)

  readonly registries$: Observable<StoreIdentity[]> = this.patch
    .watch$('serverInfo', 'registries')
    .pipe(
      map(registries =>
        Object.entries(registries).map(([url, name]) =>
          toStoreIdentity(url, name),
        ),
      ),
    )

  readonly newRegistry$ = this.registries$.pipe(
    map(registries => registries.map(registry => registry.url)),
    startWith<string[]>([]),
    pairwise(),
    mergeMap(([previous, current]) =>
      current.filter(url => !previous.some(saved => sameUrl(url, saved))),
    ),
  )

  readonly currentRegistryUrl$ = new ReplaySubject<string>(1)

  readonly requestErrors$ = new BehaviorSubject<string[]>([])

  readonly marketplace$: Observable<Marketplace> = combineLatest([
    this.newRegistry$.pipe(
      mergeMap(url =>
        this.fetchRegistry$(url).pipe(
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
    this.currentRegistry$.pipe(startWith(null)),
    this.marketplace$,
  ]).pipe(
    map(([current, marketplace]) => [
      ...(current ? [{ url: current.url, icon: current.info.icon }] : []),
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

  override fetchIcon$(registry: string, icon: string): Observable<string> {
    return this.icons.getRegistry$(registry, icon)
  }

  fetchInfo$(registry: string): Observable<T.RegistryInfo> {
    return from(this.api.rpc.registry.info({ registry })).pipe(
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

  fetchProviders$(
    url: string,
    provides: string,
    range: string,
  ): Observable<MarketplacePkg[]> {
    // Registries before 1.6.0 ignore `provides`.
    return this.fetchPackages$(url, provides, range).pipe(
      map(packages =>
        packages.filter(pkg =>
          satisfiesWithAliases(this.exver, pkg, range, provides),
        ),
      ),
    )
  }

  private fetchPackages$(
    url: string,
    provides: string | null = null,
    range: string | null = null,
  ): Observable<MarketplacePkg[]> {
    return from(
      this.api.rpc.registry.package
        .get({
          registry: url,
          id: null,
          targetVersion: range,
          provides,
          sourceVersion: null,
          otherVersions: 'short',
        })
        .then(packagesResponse),
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
      this.api.rpc.registry.package
        .get({
          registry: url,
          id,
          targetVersion: version ? `=${version}` : null,
          sourceVersion,
          otherVersions: 'short',
        })
        .then(packageResponse),
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
    pkg: Pick<MarketplacePkg, 'id' | 'version' | 's9pks' | 'signers'>,
    url: string,
    trust = false,
  ): Promise<void> {
    const keys = await firstValueFrom(
      this.patch.watch$('serverInfo', 'trustedKeys'),
    )
    await this.api.rpc.package.install({
      acceptSigners: packageSignerPolicy(pkg, keys, trust),
      id: pkg.id,
      version: pkg.version,
      registry: url,
      trust,
    })
  }

  async connect(rawUrl: string): Promise<void> {
    const url = registryUrl(rawUrl)
    this.currentRegistryUrl$.next(url)
    this.storage.set('selectedRegistry', url)
  }

  /** Resolves to the keys it newly trusted for packages. */
  async connectRegistry(url: string): Promise<string[]> {
    return this.api.rpc.server.registries.add({ url, name: null })
  }

  async add(rawUrl: string): Promise<string> {
    const url = registryUrl(rawUrl)
    await this.connectRegistry(url)
    return url
  }

  /** Resolves to the registry-added keys no connected registry still offers. */
  async removeRegistry(url: string): Promise<string[]> {
    const orphaned = await this.api.rpc.server.registries.remove({
      url,
    })
    const selected = this.storage.get<string>('selectedRegistry')

    if (selected && sameUrl(selected, url)) {
      this.storage.set('selectedRegistry', null)
    }

    return orphaned
  }

  async delete(url: string): Promise<void> {
    await this.removeRegistry(url)
  }

  /** Resolves to the keys it newly trusted for StartOS updates. */
  async setOsRegistry(url: string): Promise<string[]> {
    return this.api.rpc.server.registries.setOs({ url })
  }
}

function toStoreIdentity(url: string, name?: string | null): StoreIdentity {
  return {
    url: registryUrl(url),
    name: name || url,
  }
}
