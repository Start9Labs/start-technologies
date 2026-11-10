import { Component, computed, input } from '@angular/core'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import type { T } from '@start9labs/start-core'
import { tuiAppearance } from '@taiga-ui/core'
import { TuiBadge } from '@taiga-ui/kit'

@Component({
  selector: 'span[marketplaceRegistryStatus]',
  host: { tuiBadge: '' },
  hostDirectives: [TuiBadge],
  template: `
    {{ states[status()].label | i18n }}
  `,
  imports: [i18nPipe],
})
export class RegistryStatus {
  private readonly appearance = tuiAppearance(
    computed(() => this.states[this.status()].appearance),
  )

  protected readonly states: Record<
    T.RegistryStatus,
    { label: i18nKey; appearance: string }
  > = {
    connecting: { label: 'Connecting', appearance: 'info' },
    connected: { label: 'Connected', appearance: 'positive' },
    error: { label: 'Error', appearance: 'negative' },
  }

  public readonly status = input.required<T.RegistryStatus>({
    alias: 'marketplaceRegistryStatus',
  })
}
