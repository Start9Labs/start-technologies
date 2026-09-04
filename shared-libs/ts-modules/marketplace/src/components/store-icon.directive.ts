import { computed, Directive, inject, input, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { knownRegistries, sameUrl } from '@start9labs/shared'
import { of } from 'rxjs'

import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'

const FALLBACKS: Record<string, string> = {
  [knownRegistries.start9]: 'assets/img/icon_transparent.png',
  [knownRegistries.community]: 'assets/img/community-icon.png',
  [knownRegistries.start9Alpha]: 'assets/img/icon_alpha.png',
  [knownRegistries.start9Beta]: 'assets/img/icon_beta.png',
}

@Directive({
  selector: 'img[storeIcon]',
  host: {
    alt: '',
    '[src]': 'icon()',
    '(error)': 'onError()',
  },
})
export class StoreIconDirective {
  private readonly registryIcons = toSignal(
    inject(AbstractMarketplaceService, { optional: true })?.registryIcons$ ||
      of([]),
    { initialValue: [] },
  )

  private readonly failed = signal<ReadonlySet<string>>(new Set())

  readonly storeIcon = input<string>()

  protected readonly icon = computed(() => {
    const url = this.storeIcon() || ''
    const live = this.registryIcons().find(entry => sameUrl(entry.url, url))
    const generic = 'assets/img/storefront-outline.png'

    return (
      [
        live?.icon?.startsWith('data:image/') ? live.icon : null,
        Object.entries(FALLBACKS).find(([u]) => sameUrl(u, url))?.[1],
        generic,
      ].find(icon => icon && !this.failed().has(icon)) || generic
    )
  })

  protected onError(): void {
    const icon = this.icon()
    if (!this.failed().has(icon)) {
      this.failed.update(failed => new Set(failed).add(icon))
    }
  }
}
