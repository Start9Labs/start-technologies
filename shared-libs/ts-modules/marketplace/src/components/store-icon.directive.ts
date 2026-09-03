import { computed, Directive, inject, input, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { sameUrl } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { of } from 'rxjs'

import { pinnedIcon, resolveIcon } from '../identity'
import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'

@Directive({
  selector: 'img[storeIcon]',
  host: {
    alt: '',
    '[src]': 'icon()',
    '(error)': 'onError()',
  },
})
export class StoreIconDirective {
  private readonly marketplace = inject(AbstractMarketplaceService, {
    optional: true,
  })
  private readonly known = toSignal(
    this.marketplace?.knownRegistries$ || of<T.KnownRegistry[]>([]),
  )

  private readonly registryIcons = toSignal(
    this.marketplace?.registryIcons$ || of([]),
    { initialValue: [] },
  )

  private readonly failed = signal<ReadonlySet<string>>(new Set())

  readonly storeIcon = input<string>()

  protected readonly icon = computed(() => {
    const url = this.storeIcon() || ''
    const known = this.known()
    const live = this.registryIcons().find(entry => sameUrl(entry.url, url))
    const fallback = pinnedIcon(url, known || [])
    const generic = 'assets/img/storefront-outline.png'
    const candidates = [
      known ? resolveIcon(url, live?.icon, known) : null,
      fallback,
      generic,
    ]

    return candidates.find(icon => icon && !this.failed().has(icon)) || generic
  })

  protected onError(): void {
    const icon = this.icon()
    if (!this.failed().has(icon)) {
      this.failed.update(failed => new Set(failed).add(icon))
    }
  }
}
