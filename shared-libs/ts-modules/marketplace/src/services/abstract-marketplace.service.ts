import { Observable, of } from 'rxjs'

import { MarketplacePkg, StoreDataWithUrl, StoreIdentity } from '../types'
import { registryIconUrl } from '../util/icon'

/** Registry storage and transport supplied by the host application. */
export abstract class AbstractMarketplaceService {
  /** Icons fetched from registry info. */
  abstract readonly registryIcons$: Observable<
    { url: string; icon: string | null }[]
  >

  /** The registry currently being browsed (info + packages + url). */
  abstract readonly currentRegistry$: Observable<StoreDataWithUrl>

  /** The url of the registry currently being browsed. */
  abstract readonly currentRegistryUrl$: Observable<string>

  /** Registries saved by this app. */
  abstract readonly registries$: Observable<StoreIdentity[]>

  /** Resolve one package. version/flavor/registryUrl are optional. */
  abstract getPackage$(
    id: string,
    version: string | null,
    flavor: string | null,
    registryUrl?: string,
  ): Observable<MarketplacePkg | null>

  /** Loads a package or dependency icon through the app's transport. */
  fetchIcon$(registry: string, icon: string): Observable<string> {
    return of(registryIconUrl(registry, icon))
  }

  /** Fetch a static asset (LICENSE.md / instructions.md) for a package. */
  abstract fetchStatic$(
    pkg: MarketplacePkg,
    path: 'LICENSE.md' | 'instructions.md',
  ): Observable<string>

  /** Switch the browsed registry to `url` and persist the selection. */
  abstract connect(url: string): Promise<void>

  /** Saves a registry and returns its normalized URL. */
  abstract add(rawUrl: string): Promise<string>

  /** Removes a saved registry. */
  abstract delete(url: string): Promise<void>
}
