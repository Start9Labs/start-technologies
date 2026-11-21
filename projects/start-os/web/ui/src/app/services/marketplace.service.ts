import { inject, Injectable } from '@angular/core'
import {
  AbstractMarketplaceService,
  Marketplace,
  MarketplacePkg,
  StoreDataWithUrl,
  StoreIdentity,
  packageSignerPolicy,
  registryStatus,
  isRegistryLoading,
} from '@start9labs/marketplace'
import {
  defaultRegistries,
  Exver,
  registryUrl,
  sameUrl,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  combineLatest,
  distinctUntilChanged,
  filter,
  from,
  firstValueFrom,
  map,
  Observable,
  of,
  ReplaySubject,
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

  private readonly registryCache$ = this.patch.watch$('registryCache')

  /** Lists defaults in `defaultRegistries` order, then custom registries in saved order. */
  readonly registries$: Observable<
    (StoreIdentity & { status: T.RegistryStatus })[]
  > = combineLatest([
    this.patch.watch$('serverInfo', 'registries'),
    this.registryCache$,
  ]).pipe(
    map(([registries, cache]) =>
      Object.entries(registries)
        .sort(([a], [b]) => defaultRank(a) - defaultRank(b))
        .flatMap(([url, name]) => {
          try {
            const entry = findRegistryCache(cache, url)
            return [
              {
                ...toStoreIdentity(url, name),
                status: registryStatus(entry),
                error: entry?.error,
              },
            ]
          } catch {
            return []
          }
        }),
    ),
  )

  private watchRegistry$(url: string) {
    return this.registryCache$.pipe(
      map(cache => findRegistryCache(cache, url)),
      distinctUntilChanged(),
    )
  }

  readonly currentRegistryUrl$ = new ReplaySubject<string>(1)

  readonly requestErrors$ = this.registries$.pipe(
    map(registries =>
      registries
        .filter(registry => registry.status === 'error')
        .map(({ url }) => url),
    ),
  )

  readonly marketplace$: Observable<Marketplace> = combineLatest([
    this.registries$,
    this.registryCache$,
  ]).pipe(
    map(([registries, cache]) =>
      Object.fromEntries(
        registries.map(({ url }) => {
          const entry = findRegistryCache(cache, url)
          return [url, entry ? this.toStore(url, entry) : null]
        }),
      ),
    ),
    shareReplay(1),
  )

  readonly currentRegistry$: Observable<StoreDataWithUrl> =
    this.currentRegistryUrl$.pipe(
      distinctUntilChanged(),
      switchMap(url =>
        this.watchRegistry$(url).pipe(
          filter(entry => !!entry),
          map(entry => this.toStore(url, entry)),
        ),
      ),
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
    return (registryUrl ? of(registryUrl) : this.currentRegistryUrl$).pipe(
      distinctUntilChanged(),
      switchMap(url => this.watchRegistry$(url)),
      filter(registryReady),
      map(entry => {
        const pkg = entry.catalog[id]
        if (!pkg) return null
        const listed = listedVersions(this.exver, pkg)
        const selected =
          version ?? listed.find(v => this.exver.getFlavor(v) === flavor)
        return selected
          ? toMarketplacePkg(this.exver, id, selected, pkg, listed)
          : null
      }),
    )
  }

  override fetchIcon$(registry: string, icon: string): Observable<string> {
    return this.icons.getRegistry$(registry, icon)
  }

  fetchInfo$(registry: string): Observable<T.RegistryInfo> {
    return this.watchRegistry$(registry).pipe(
      filter(registryReady),
      map(entry => this.registryInfo(entry.info)),
    )
  }

  private registryInfo(info: T.RegistryInfo): T.RegistryInfo {
    return { ...info, categories: { all: { name: 'All' }, ...info.categories } }
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

  private toStore(
    url: string,
    entry: T.Public['registryCache'][string],
  ): StoreDataWithUrl {
    const packages = Object.entries(entry.catalog).flatMap(([id, pkg]) => {
      const listed = listedVersions(this.exver, pkg)
      return listed.flatMap(
        version => toMarketplacePkg(this.exver, id, version, pkg, listed) ?? [],
      )
    })
    return {
      url,
      info: this.registryInfo(entry.info),
      status: registryStatus(entry),
      error: entry.error,
      packages,
      updates: packages.filter(
        pkg => entry.catalog[pkg.id]?.updateTarget === pkg.version,
      ),
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

  async refreshRegistry(url: string): Promise<void> {
    await this.api.rpc.server.registries.refresh({ url })
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

  /** Resolves to the keys it newly trusted for StartOS updates. */
  async setOsRegistry(url: string): Promise<string[]> {
    return this.api.rpc.server.registries.setOs({ url })
  }
}

/** The release listed for each flavor: its update target, else its newest. */
function listedVersions(exver: Exver, pkg: T.PackageReleases): string[] {
  const listed = new Map<string | null, string>()
  for (const version of Object.keys(pkg.versions)) {
    const flavor = exver.getFlavor(version)
    const newest = listed.get(flavor)
    if (!newest || exver.compareExver(version, newest) === 1) {
      listed.set(flavor, version)
    }
  }
  if (pkg.updateTarget) {
    listed.set(exver.getFlavor(pkg.updateTarget), pkg.updateTarget)
  }
  return [...listed.values()]
}

function toMarketplacePkg(
  exver: Exver,
  id: string,
  version: string,
  pkg: T.PackageReleases,
  listed: string[],
): MarketplacePkg | null {
  const release = pkg.versions[version]
  if (!release) return null
  const { superseded, ...info } = release
  return {
    id,
    flavor: exver.getFlavor(version),
    version,
    categories: pkg.categories,
    firstPublishedAt: pkg.firstPublishedAt,
    best: Object.fromEntries(
      listed.flatMap(v => {
        const best = pkg.versions[v]
        if (!best) return []
        const { superseded, ...bestInfo } = best
        return [[v, bestInfo]]
      }),
    ),
    otherVersions: Object.fromEntries(
      Object.entries(pkg.versions)
        .filter(([v]) => v !== version)
        .map(([v, r]) => [
          v,
          {
            releaseNotes: r.releaseNotes,
            defaultLocale: r.defaultLocale,
            superseded: r.superseded,
          },
        ]),
    ),
    ...info,
  }
}

function toStoreIdentity(url: string, name?: string | null): StoreIdentity {
  return {
    url: registryUrl(url),
    name: name || url,
  }
}

function registryReady(
  entry: T.RegistryCache | undefined,
): entry is T.RegistryCache {
  return (
    !!entry &&
    !isRegistryLoading(
      registryStatus(entry),
      Object.keys(entry.catalog ?? {}).length,
    )
  )
}

/** Browser snapshots can predate `RegistryCache.catalog`. */
function findRegistryCache(cache: DataModel['registryCache'], url: string) {
  const entry = Object.entries(cache).find(([saved]) =>
    sameUrl(saved, url),
  )?.[1]
  return entry && !entry.catalog ? { ...entry, catalog: {} } : entry
}

function defaultRank(url: string): number {
  const defaults = Object.values(defaultRegistries)
  const rank = defaults.findIndex(known => sameUrl(known, url))
  return rank < 0 ? defaults.length : rank
}
