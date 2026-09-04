import { Observable } from 'rxjs'

import { MarketplacePkg, StoreDataWithUrl, StoreIdentity } from '../types'

/**
 * Contract the shared marketplace components (tile, preview, registry-select)
 * depend on. Each app provides a concrete implementation: the embedded OS UI
 * persists registries to patch-db; the public brochure persists them to
 * localStorage and hits registries' RPC directly.
 */
export abstract class AbstractMarketplaceService {
  /** Icons fetched from registry info. */
  abstract readonly registryIcons$: Observable<
    { url: string; icon: string | null }[]
  >

  /** The registry currently being browsed (info + packages + url). */
  abstract readonly currentRegistry$: Observable<StoreDataWithUrl>

  /** The url of the registry currently being browsed. */
  abstract readonly currentRegistryUrl$: Observable<string>

  /** All registries known to this app (start9, community, then customs). */
  abstract readonly registries$: Observable<StoreIdentity[]>

  /** Resolve one package. version/flavor/registryUrl are optional. */
  abstract getPackage$(
    id: string,
    version: string | null,
    flavor: string | null,
    registryUrl?: string,
  ): Observable<MarketplacePkg | null>

  /** Fetch a static asset (LICENSE.md / instructions.md) for a package. */
  abstract fetchStatic$(
    pkg: MarketplacePkg,
    path: 'LICENSE.md' | 'instructions.md',
  ): Observable<string>

  /** Switch the browsed registry to `url` and persist the selection. */
  abstract connect(url: string): Promise<void>

  /** Validate & persist a custom registry; resolves to its normalized url. */
  abstract add(rawUrl: string): Promise<string>

  /** Remove a custom registry. */
  abstract delete(url: string): Promise<void>
}
