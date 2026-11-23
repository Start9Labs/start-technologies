import { inject, Injectable } from '@angular/core'
import {
  AbstractMarketplaceService,
  Marketplace,
  MarketplacePkg,
  StoreDataWithUrl,
  StoreIdentity,
  packageResponse,
  packageSignerPolicy,
  registryStatus,
  isRegistryLoading,
} from '@start9labs/marketplace'
import {
  ErrorService,
  Exver,
  isNetworkError,
  registryUrl,
  sameUrl,
  withAbortSignal,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  catchError,
  combineLatest,
  defer,
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
import { ConnectionService } from 'src/app/services/connection.service'
import { PackageIconService } from 'src/app/services/package-icon.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { StorageService } from 'src/app/services/storage.service'
import { sortedRegistries } from 'src/app/utils/registries'

@Injectable({
  providedIn: 'root',
})
export class MarketplaceService extends AbstractMarketplaceService {
  private readonly api = inject(ApiService)
  private readonly icons = inject(PackageIconService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly exver = inject(Exver)
  private readonly storage = inject(StorageService)
  private readonly connection = inject(ConnectionService)
  private readonly errors = inject(ErrorService)

  private readonly registryCache$ = this.patch.watch$('registryCache')

  /** Registries in {@link sortedRegistries} order. */
  readonly registries$: Observable<
    (StoreIdentity & { status: T.RegistryStatus })[]
  > = combineLatest([
    this.patch.watch$('serverInfo', 'registries'),
    this.registryCache$,
  ]).pipe(
    map(([registries, cache]) =>
      sortedRegistries(registries).flatMap(([url, name]) => {
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

  private watchListing$(registry: string, id: string) {
    return this.watchRegistry$(registry).pipe(
      filter(registryReady),
      map(entry => entry.listings[id]),
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

  /** Listed releases follow the live cache; any other version follows its server stream. */
  getPackage$(
    id: string,
    version: string | null,
    flavor: string | null,
    registryUrl?: string,
  ): Observable<MarketplacePkg | null> {
    return (registryUrl ? of(registryUrl) : this.currentRegistryUrl$).pipe(
      distinctUntilChanged(),
      switchMap(url =>
        this.watchListing$(url, id).pipe(
          map(row => {
            const selected =
              version ?? (row && newestReleases(this.exver, row)[flavor ?? ''])
            return row && selected ? { row, selected } : null
          }),
          distinctUntilChanged(
            (a, b) =>
              !!a &&
              !!b &&
              a.selected === b.selected &&
              !a.row.releases[a.selected] &&
              !b.row.releases[b.selected],
          ),
          switchMap(listing => {
            if (!listing) return of(null)
            const { row, selected } = listing
            const release = row.releases[selected]
            return release
              ? of(toPkg(this.exver, id, selected, row, release))
              : this.watchDetails$(id, selected, url)
          }),
        ),
      ),
    )
  }

  /** A release with every other version's release notes, following live package changes. */
  getDetails$(
    id: string,
    version: string,
    registry: string,
  ): Observable<MarketplacePkg | null> {
    return this.watchListing$(registry, id).pipe(
      map(row => !!row),
      distinctUntilChanged(),
      switchMap(listed =>
        listed ? this.watchDetails$(id, version, registry) : of(null),
      ),
    )
  }

  /** The server streams each change to these details; a failure resubscribes once connected. */
  private watchDetails$(
    id: string,
    version: string,
    registry: string,
  ): Observable<MarketplacePkg | null> {
    return this.connection.pipe(
      filter(Boolean),
      switchMap(() =>
        withAbortSignal(signal =>
          defer(() =>
            this.api.streams.server.registries.package(
              { registry, id, targetVersion: `=${version}` },
              { signal },
            ),
          ).pipe(
            switchMap(({ initial, items }) =>
              from(items).pipe(startWith(initial)),
            ),
          ),
        ).pipe(
          map(details => this.toDetails(details, id, version)),
          catchError(error => {
            if (!isNetworkError(error)) this.errors.handleError(error)
            return of(null)
          }),
        ),
      ),
    )
  }

  private toDetails(
    details: T.GetPackageResponse,
    id: string,
    version: string,
  ): MarketplacePkg | null {
    const pkg = packageResponse(details)
    const best = pkg.best[version]
    return best
      ? {
          ...pkg,
          ...best,
          id,
          version,
          flavor: this.exver.getFlavor(version),
        }
      : null
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
    const listings = Object.entries(entry.listings)
    return {
      url,
      info: this.registryInfo(entry.info),
      status: registryStatus(entry),
      error: entry.error,
      packages: listings.flatMap(([id, row]) =>
        Object.values(newestReleases(this.exver, row)).map(version =>
          toPkg(this.exver, id, version, row, row.releases[version]!),
        ),
      ),
      updates: listings
        .flatMap(([id, row]) => {
          const release = row.updateTarget && row.releases[row.updateTarget]
          return release && row.updateTarget
            ? [toPkg(this.exver, id, row.updateTarget, row, release)]
            : []
        })
        .sort(
          (a, b) => a.title.localeCompare(b.title) || a.id.localeCompare(b.id),
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

/** The newest listed release of each flavor, keyed by flavor (`''` for none). */
function newestReleases(
  exver: Exver,
  row: T.PackageListing,
): Record<string, string> {
  const newest: Record<string, string> = {}
  Object.keys(row.releases).forEach(version => {
    const flavor = exver.getFlavor(version) ?? ''
    const current = newest[flavor]
    if (!current || exver.compareExver(version, current) === 1)
      newest[flavor] = version
  })
  return newest
}

function toPkg(
  exver: Exver,
  id: string,
  version: string,
  { categories, firstPublishedAt, releases, versions }: T.PackageListing,
  release: T.PackageVersionInfo,
): MarketplacePkg {
  return {
    ...release,
    id,
    version,
    flavor: exver.getFlavor(version),
    categories,
    firstPublishedAt,
    best: { [version]: release },
    otherVersions: Object.fromEntries(
      Object.entries(versions)
        .filter(([other]) => other !== version)
        .map(([other, info]) => {
          const listed = releases[other]
          return [
            other,
            listed
              ? {
                  ...info,
                  releaseNotes: listed.releaseNotes,
                  defaultLocale: listed.defaultLocale,
                }
              : info,
          ]
        }),
    ),
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
      Object.keys(entry.listings).length,
    )
  )
}

/** Browser snapshots can predate `RegistryCache.listings`. */
function findRegistryCache(cache: DataModel['registryCache'], url: string) {
  const entry = Object.entries(cache).find(([saved]) =>
    sameUrl(saved, url),
  )?.[1]
  return entry && !entry.listings ? { ...entry, listings: {} } : entry
}
