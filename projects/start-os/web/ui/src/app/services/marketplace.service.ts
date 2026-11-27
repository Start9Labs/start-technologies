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
import {
  PatchDB,
  pathFromArray,
  Projection,
  SubtreePatchDB,
} from 'patch-db-client'
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
import { affecting, FoundVulnerability } from 'src/app/utils/vulnerabilities'

@Injectable({
  providedIn: 'root',
})
export class MarketplaceService extends AbstractMarketplaceService {
  private readonly api = inject(ApiService)
  private readonly icons = inject(PackageIconService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly index = this.patch as SubtreePatchDB<DataModel>
  private readonly exver = inject(Exver)
  private readonly storage = inject(StorageService)

  private readonly registryCache$ = this.patch.watch$('registryCache')

  /** Listed releases, reused while a package's projection and selection are unchanged. */
  private readonly listed = new WeakMap<
    Partial<T.PackageInfo>,
    { selection: Selection; releases: Map<string, MarketplacePkg> }
  >()

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
            const entry = findRegistryCache(cache, url)?.[1]
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
      distinctUntilChanged((a, b) => a?.[0] === b?.[0] && a?.[1] === b?.[1]),
    )
  }

  /** Trusted vulnerabilities the registry publishes for the package that affect a version. */
  vulnerabilities$(
    registry: string,
    id: string,
    version: string,
  ): Observable<FoundVulnerability[]> {
    return this.watchSelection$(registry, id).pipe(
      switchMap(({ key, selection }) =>
        selection?.vulnerabilities?.length
          ? this.watchPackages$(
              key,
              {
                include: selection.vulnerabilities.map(v =>
                  pathFromArray(['vulnerabilities', v]),
                ),
              },
              id,
            ).pipe(
              map((info: Partial<T.PackageInfo> | null) =>
                affecting(
                  info?.vulnerabilities,
                  selection.vulnerabilities,
                  range => this.exver.satisfies(version, range),
                ),
              ),
            )
          : of([]),
      ),
    )
  }

  private watchSelection$(registry: string, id: string) {
    return this.watchRegistry$(registry).pipe(
      filter((found): found is Saved => !!found && registryReady(found[1])),
      map(([key, entry]) => ({ key, selection: entry.selections[id] })),
      distinctUntilChanged(
        (a, b) => a.key === b.key && a.selection === b.selection,
      ),
    )
  }

  /** A projection of the registry's saved packages, keyed by package id. */
  private watchPackages$(
    key: string,
    projection: Projection,
    ...path: string[]
  ): Observable<any> {
    return this.index.watchProjection$(
      projection,
      'registryIndexes',
      key,
      'package',
      'packages',
      ...path,
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

  readonly marketplace$: Observable<Marketplace> = this.registries$.pipe(
    map(registries => registries.map(({ url }) => url)),
    distinctUntilChanged((a, b) => a.join() === b.join()),
    switchMap(urls =>
      urls.length
        ? combineLatest(
            urls.map(url =>
              this.watchStore$(url).pipe(
                map(store => [url, store] as const),
                startWith([url, null] as const),
              ),
            ),
          )
        : of([]),
    ),
    map(entries => Object.fromEntries(entries)),
    shareReplay(1),
  )

  readonly currentRegistry$: Observable<StoreDataWithUrl> =
    this.currentRegistryUrl$.pipe(
      distinctUntilChanged(),
      switchMap(url =>
        this.watchStore$(url).pipe(
          filter((store): store is StoreDataWithUrl => !!store),
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

  /**
   * The registry's listing: each package's selected releases without long descriptions.
   * Nothing until those releases arrive, so an unavailable listing never appears empty.
   */
  private watchStore$(url: string): Observable<StoreDataWithUrl | null> {
    const entry$ = this.watchRegistry$(url).pipe(
      shareReplay({ bufferSize: 1, refCount: true }),
    )
    return entry$.pipe(
      map(found => found && { key: found[0], include: listedPaths(found[1]) }),
      distinctUntilChanged(
        (a, b) =>
          a?.key === b?.key && a?.include.join('\n') === b?.include.join('\n'),
      ),
      switchMap(listing => {
        if (!listing) return of(null)
        const packages$ = listing.include.length
          ? this.watchPackages$(listing.key, {
              include: [...listing.include, '/*/categories'],
              exclude: ['/*/versions/*/description/long'],
            })
          : of({})
        return packages$.pipe(
          switchMap(packages =>
            entry$.pipe(
              map(found =>
                found ? this.toStore(url, found[1], packages ?? {}) : null,
              ),
            ),
          ),
        )
      }),
    )
  }

  /** A release of a package, following live registry changes. */
  getPackage$(
    id: string,
    version: string | null,
    flavor: string | null,
    registryUrl?: string,
  ): Observable<MarketplacePkg | null> {
    return (registryUrl ? of(registryUrl) : this.currentRegistryUrl$).pipe(
      distinctUntilChanged(),
      switchMap(url => this.watchRelease$(id, version, flavor, url, false)),
    )
  }

  /** A release with every version's release notes, following live registry changes. */
  getDetails$(
    id: string,
    version: string,
    registry: string,
  ): Observable<MarketplacePkg | null> {
    return this.watchRelease$(id, version, null, registry, true)
  }

  private watchRelease$(
    id: string,
    version: string | null,
    flavor: string | null,
    registry: string,
    notes: boolean,
  ): Observable<MarketplacePkg | null> {
    return this.watchSelection$(registry, id).pipe(
      switchMap(({ key, selection }) => {
        const selected =
          version ??
          (selection &&
            newestReleases(this.exver, selection.best)[flavor ?? ''])
        if (!selection || !selected || !selection.versions[selected])
          return of(null)
        return this.watchPackages$(
          key,
          {
            include: [
              pathFromArray(['versions', selected]),
              '/categories',
              ...(notes
                ? ['/versions/*/releaseNotes', '/versions/*/defaultLocale']
                : []),
            ],
          },
          id,
        ).pipe(
          map((info: Partial<T.PackageInfo> | null) => {
            const release = info?.versions?.[selected]
            return release
              ? toPkg(
                  this.exver,
                  id,
                  selected,
                  selection,
                  info.categories ?? [],
                  release,
                  info.versions ?? {},
                )
              : null
          }),
        )
      }),
    )
  }

  override fetchIcon$(registry: string, icon: string): Observable<string> {
    return this.icons.getRegistry$(registry, icon)
  }

  fetchInfo$(registry: string): Observable<T.RegistryInfo> {
    return this.watchRegistry$(registry).pipe(
      filter((found): found is Saved => !!found && registryReady(found[1])),
      map(([, entry]) => this.registryInfo(entry.info)),
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
    entry: T.RegistryCache,
    packages: Record<string, Partial<T.PackageInfo>>,
  ): StoreDataWithUrl {
    const release = (id: string, version: string) => {
      const selection = entry.selections[id]
      const info = packages[id]
      const found = info?.versions?.[version]
      if (!selection || !found) return []
      let listed = this.listed.get(info)
      if (listed?.selection !== selection)
        this.listed.set(info, (listed = { selection, releases: new Map() }))
      let pkg = listed.releases.get(version)
      if (!pkg) {
        pkg = toPkg(
          this.exver,
          id,
          version,
          selection,
          info.categories ?? [],
          found,
          info.versions ?? {},
        )
        listed.releases.set(version, pkg)
      }
      return [pkg]
    }
    const selections = Object.entries(entry.selections)
    return {
      url,
      info: this.registryInfo(entry.info),
      status: registryStatus(entry),
      error: entry.error,
      packages: selections.flatMap(([id, selection]) =>
        selection.best.flatMap(version => release(id, version)),
      ),
      updates: selections.flatMap(([id, selection]) =>
        selection.updateTarget ? release(id, selection.updateTarget) : [],
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

type Selection = T.RegistryCache['selections'][string]
type Saved = [string, T.RegistryCache]

/** The index paths of the releases a listing shows. */
function listedPaths(entry: T.RegistryCache): string[] {
  return Object.entries(entry.selections).flatMap(([id, selection]) =>
    listedVersions(selection).map(version =>
      pathFromArray([id, 'versions', version]),
    ),
  )
}

/** The releases a listing shows: the newest of each flavor and the update target. */
function listedVersions(selection: Selection): string[] {
  return [
    ...new Set([
      ...selection.best,
      ...(selection.updateTarget ? [selection.updateTarget] : []),
    ]),
  ]
}

/** The newest selected release of each flavor, keyed by flavor (`''` for none). */
function newestReleases(
  exver: Exver,
  versions: readonly string[],
): Record<string, string> {
  const newest: Record<string, string> = {}
  versions.forEach(version => {
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
  { firstPublishedAt, versions }: Selection,
  categories: readonly string[],
  release: T.PackageVersionInfo,
  releases: Record<string, Partial<T.PackageVersionInfo>>,
): MarketplacePkg {
  const selected = {
    ...release,
    s9pks: (versions[version]?.s9pks ?? []).flatMap(index =>
      release.s9pks.slice(index, index + 1),
    ),
  }
  return {
    ...selected,
    id,
    version,
    flavor: exver.getFlavor(version),
    categories: [...categories],
    firstPublishedAt: firstPublishedAt ?? null,
    best: { [version]: selected },
    otherVersions: Object.fromEntries(
      Object.entries(versions)
        .filter(([other]) => other !== version)
        .map(([other, { superseded }]) => {
          const notes = releases[other]
          return [
            other,
            {
              ...(superseded ? { superseded } : {}),
              ...(notes?.releaseNotes
                ? {
                    releaseNotes: notes.releaseNotes,
                    defaultLocale: notes.defaultLocale,
                  }
                : {}),
            },
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

function registryReady(entry: T.RegistryCache): boolean {
  return !isRegistryLoading(
    registryStatus(entry),
    Object.keys(entry.selections).length,
  )
}

/** The saved key and cache entry of a registry; snapshots can predate `selections`. */
function findRegistryCache(
  cache: DataModel['registryCache'],
  url: string,
): Saved | undefined {
  const found = Object.entries(cache).find(([saved]) => sameUrl(saved, url))
  if (!found) return undefined
  const [key, entry] = found
  return [key, entry.selections ? entry : { ...entry, selections: {} }]
}

function defaultRank(url: string): number {
  const defaults = Object.values(defaultRegistries)
  const rank = defaults.findIndex(known => sameUrl(known, url))
  return rank < 0 ? defaults.length : rank
}
