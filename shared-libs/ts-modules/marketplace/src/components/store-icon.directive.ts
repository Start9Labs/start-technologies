import { computed, Directive, inject, input, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { registriesSnapshot, sameUrl } from '@start9labs/shared'
import { of } from 'rxjs'

import { resolveRegistry } from '../identity'
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
    this.marketplace?.knownRegistries$ || of(registriesSnapshot),
    { initialValue: registriesSnapshot },
  )

  private readonly registryIcons = toSignal(
    this.marketplace?.registryIcons$ || of([]),
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
        resolveRegistry(url, { icon: live?.icon }, this.known()).icon,
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
