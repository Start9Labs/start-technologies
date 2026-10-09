import { computed, Directive, inject, input, signal } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import { catchError, defer, of, startWith, switchMap } from 'rxjs'

import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'
import { MarketplacePkgBase } from '../types'
import { FALLBACK_ICON, registryIconUrl } from '../util/icon'

@Directive({
  selector: 'img[marketplaceIcon]',
  host: {
    alt: '',
    '[src]': 'failed() === src() ? fallback : src()',
    '(error)': 'failed.set(src())',
  },
})
export class MarketplaceIconDirective {
  private readonly marketplace = inject(AbstractMarketplaceService, {
    optional: true,
  })
  private readonly current = toSignal(
    this.marketplace?.currentRegistryUrl$ || of(null),
  )

  readonly marketplaceIcon = input.required<MarketplacePkgBase>()
  readonly dependency = input<string>()
  readonly registry = input<string>()

  protected readonly fallback = FALLBACK_ICON
  protected readonly failed = signal<string | null>(null)

  protected readonly src = toSignal(
    toObservable(
      computed(() => ({
        pkg: this.marketplaceIcon(),
        dependency: this.dependency(),
        registry: this.registry() || this.current(),
      })),
    ).pipe(
      switchMap(({ pkg, dependency, registry }) =>
        defer(() => {
          const inline = dependency
            ? pkg.dependencyMetadata[dependency]?.icon
            : pkg.icon

          if (inline || !registry) return of(inline || FALLBACK_ICON)

          return this.marketplace
            ? this.marketplace.fetchIcon$(
                registry,
                pkg.id,
                pkg.version,
                dependency,
              )
            : of(registryIconUrl(registry, pkg.id, pkg.version, dependency))
        }).pipe(
          catchError(() => of(FALLBACK_ICON)),
          startWith(FALLBACK_ICON),
        ),
      ),
    ),
    { initialValue: FALLBACK_ICON },
  )
}
