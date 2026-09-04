import { inject, Injectable } from '@angular/core'
import { WA_LOCAL_STORAGE } from '@ng-web-apis/common'
import {
  AbstractMarketplaceService,
  GetPackageRes,
  MarketplacePkg,
  StoreDataWithUrl,
  StoreIdentity,
} from '@start9labs/marketplace'
import {
  defaultRegistries,
  Exver,
  i18nPipe,
  i18nService,
  registryUrl,
  sameUrl,
  toUrl,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  BehaviorSubject,
  combineLatest,
  firstValueFrom,
  from,
  Observable,
  of,
  ReplaySubject,
  Subject,
} from 'rxjs'
import {
  catchError,
  distinctUntilChanged,
  filter,
  map,
  shareReplay,
  startWith,
  switchMap,
  tap,
} from 'rxjs/operators'

import { ApiService } from './api.service'
import { Localize, localizePackageRes, localizeRegistryInfo } from './localize'

const { start9, community } = defaultRegistries

const PREFIX = '_startos/'
const CUSTOM_KEY = 'customRegistries'
const SELECTED_KEY = 'selectedRegistry'

@Injectable({
  providedIn: 'root',
})
export class MarketplaceService extends AbstractMarketplaceService {
  private readonly api = inject(ApiService)
  private readonly exver = inject(Exver)
  private readonly storage = inject(WA_LOCAL_STORAGE)
  private readonly i18n = inject(i18nPipe)
  private readonly i18nService = inject(i18nService)

  // Collapse LocaleString metadata to the active locale as it enters the app —
  // see localize.ts for why this static site must do it client-side.
  private readonly localize: Localize = value =>
    this.i18nService.localize(value)

  // Emits the url of a registry that failed to load (unreachable / CORS).
  readonly registryError$ = new Subject<string>()

  // Custom registries live in localStorage; back them with a subject so the
  // picker updates reactively (localStorage itself is not observable).
  private readonly custom$ = new BehaviorSubject<Record<string, string | null>>(
    this.read<Record<string, string | null>>(CUSTOM_KEY) || {},
  )

  readonly registries$: Observable<StoreIdentity[]> = this.custom$.pipe(
    map(custom => [
      { url: start9, name: 'Start9 Registry' },
      { url: community, name: 'Community Registry' },
      ...Object.entries(custom)
        .filter(([u]) => !sameUrl(u, start9) && !sameUrl(u, community))
        .map(([url, name]) => ({ url, name: name || url })),
    ]),
    shareReplay(1),
  )

  readonly currentRegistryUrl$ = new ReplaySubject<string>(1)

  private readonly fetched = new Map<string, StoreDataWithUrl>()

  // Fetches any url, saved or not, so a deep link to an unsaved registry loads.
  readonly currentRegistry$: Observable<StoreDataWithUrl> =
    this.currentRegistryUrl$.pipe(
      distinctUntilChanged((a: string, b: string) => sameUrl(a, b)),
      switchMap(url =>
        this.fetchRegistry$(url).pipe(startWith(this.fetched.get(toUrl(url)))),
      ),
      filter((r): r is StoreDataWithUrl => !!r),
      tap(reg => {
        this.fetched.set(toUrl(reg.url), reg)
        this.cacheName(reg.url, reg.info.name)
      }),
      shareReplay(1),
    )

  readonly registryIcons$ = this.currentRegistry$.pipe(
    map(() =>
      [...this.fetched.values()].map(({ url, info }) => ({
        url,
        icon: info.icon,
      })),
    ),
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

  fetchStatic$(
    pkg: MarketplacePkg,
    path: 'LICENSE.md' | 'instructions.md',
  ): Observable<string> {
    return from(this.api.getStaticProxy(pkg, path))
  }

  /** The registry selected in a prior session, if any (used as the default). */
  get lastSelectedRegistry(): string | null {
    return this.read<string>(SELECTED_KEY)
  }

  async connect(url: string): Promise<void> {
    this.currentRegistryUrl$.next(url)
    this.write(SELECTED_KEY, url)
  }

  async add(rawUrl: string): Promise<string> {
    const url = registryUrl(rawUrl)

    if (
      (await firstValueFrom(this.registries$)).some(r => sameUrl(r.url, url))
    ) {
      throw new Error(this.i18n.transform('Registry already added'))
    }

    // validates the registry is reachable and provides a display name
    const info = await firstValueFrom(this.fetchInfo$(url))
    this.setCustom({ ...this.custom$.value, [url]: info.name })

    return url
  }

  async delete(url: string): Promise<void> {
    this.setCustom(
      Object.fromEntries(
        Object.entries(this.custom$.value).filter(([u]) => !sameUrl(u, url)),
      ),
    )

    // If the deleted registry was the persisted selection, clear it so the next
    // no-?registry load falls back to the default rather than the deleted one.
    const selected = this.read<string>(SELECTED_KEY)
    if (selected && sameUrl(selected, url)) {
      this.write(SELECTED_KEY, null)
    }
  }

  fetchInfo$(url: string): Observable<T.RegistryInfo> {
    return from(this.api.getRegistryInfo(url)).pipe(
      map(info => localizeRegistryInfo(info, this.localize)),
      map(info => ({
        ...info,
        categories: { all: { name: 'All' }, ...info.categories },
      })),
    )
  }

  private fetchRegistry$(url: string): Observable<StoreDataWithUrl | null> {
    return combineLatest([this.fetchInfo$(url), this.fetchPackages$(url)]).pipe(
      map(([info, packages]) => ({ info, packages, url })),
      catchError(e => {
        console.error(e)
        this.registryError$.next(url)
        return of(null)
      }),
    )
  }

  private fetchPackages$(url: string): Observable<MarketplacePkg[]> {
    return from(this.api.getRegistryPackages(url)).pipe(
      map(packages =>
        Object.entries(packages).flatMap(([id, raw]) => {
          const pkgInfo = localizePackageRes(raw, this.localize)
          return Object.keys(pkgInfo.best).flatMap(version => {
            const pkg = this.convertToMarketplacePkg(
              id,
              version,
              this.exver.getFlavor(version),
              pkgInfo,
            )
            return pkg ? [pkg] : []
          })
        }),
      ),
    )
  }

  private fetchPackage$(
    url: string,
    id: string,
    version: string | null,
    flavor: string | null,
  ): Observable<MarketplacePkg | null> {
    return from(
      this.api.getRegistryPackage(url, id, version ? `=${version}` : null),
    ).pipe(
      map(pkgInfo =>
        this.convertToMarketplacePkg(
          id,
          version,
          flavor,
          localizePackageRes(pkgInfo, this.localize),
        ),
      ),
    )
  }

  private convertToMarketplacePkg(
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

  private cacheName(url: string, name: string | null): void {
    const key = Object.keys(this.custom$.value).find(u => sameUrl(u, url))
    if (key && this.custom$.value[key] !== name) {
      this.setCustom({ ...this.custom$.value, [key]: name })
    }
  }

  private setCustom(value: Record<string, string | null>): void {
    this.write(CUSTOM_KEY, value)
    this.custom$.next(value)
  }

  private read<T>(key: string): T | null {
    try {
      return JSON.parse(this.storage?.getItem(`${PREFIX}${key}`) || 'null')
    } catch {
      return null
    }
  }

  private write(key: string, value: unknown): void {
    this.storage?.setItem(`${PREFIX}${key}`, JSON.stringify(value))
  }
}
