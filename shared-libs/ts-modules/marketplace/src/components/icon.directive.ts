import { computed, Directive, inject, input, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { of } from 'rxjs'

import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'
import { MarketplacePkgBase } from '../types'

export const FALLBACK_ICON = 'assets/img/service-icons/fallback.png'

export function registryIconUrl(
  registry: string,
  id: string,
  version: string,
  dependency?: string,
): string {
  const path = [
    id,
    version,
    ...(dependency ? ['dependencies', dependency] : []),
  ]
    .map(encodeURIComponent)
    .join('/')

  return new URL(`icons/${path}`, registry).href
}

/** Registries omit inline icons only for StartOS 0.4.0.3 and later. */
@Directive({
  selector: 'img[marketplaceIcon]',
  host: {
    alt: '',
    '[src]': 'failed() === src() ? fallback : src()',
    '(error)': 'failed.set(src())',
  },
})
export class MarketplaceIconDirective {
  private readonly current = toSignal(
    inject(AbstractMarketplaceService, { optional: true })
      ?.currentRegistryUrl$ || of(null),
  )

  readonly marketplaceIcon = input.required<MarketplacePkgBase>()
  readonly dependency = input<string>()
  readonly registry = input<string>()

  protected readonly fallback = FALLBACK_ICON
  protected readonly failed = signal<string | null>(null)

  protected readonly src = computed(() => {
    const pkg = this.marketplaceIcon()
    const dependency = this.dependency()
    const registry = this.registry() || this.current()
    const inline = dependency
      ? pkg.dependencyMetadata[dependency]?.icon
      : pkg.icon

    return (
      inline ||
      (registry
        ? registryIconUrl(registry, pkg.id, pkg.version, dependency)
        : FALLBACK_ICON)
    )
  })
}
