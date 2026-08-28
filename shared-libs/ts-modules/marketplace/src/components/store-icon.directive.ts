import { computed, Directive, inject, input } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { T } from '@start9labs/start-core'
import { of } from 'rxjs'

import { pinnedIcon } from '../identity'
import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'

@Directive({
  selector: 'img[storeIcon]',
  host: {
    alt: '',
    '[src]': 'icon()',
  },
})
export class StoreIconDirective {
  private readonly known = toSignal(
    inject(AbstractMarketplaceService, { optional: true })?.knownRegistries$ ||
      of<T.KnownRegistry[]>([]),
    { initialValue: [] },
  )

  readonly storeIcon = input<string>()

  protected readonly icon = computed(
    () =>
      pinnedIcon(this.storeIcon() || '', this.known()) ||
      'assets/img/storefront-outline.png',
  )
}
