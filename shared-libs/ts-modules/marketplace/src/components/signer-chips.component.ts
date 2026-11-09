import { Component, inject, input } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { TuiChip } from '@taiga-ui/kit'
import { MarketplaceTrust, SignerStatusesPipe, TrustContext } from '../trust'

@Component({
  selector: 'marketplace-signer-chips',
  template: `
    @for (
      signer of keys() | signerStatuses: trust.keys() ?? {} : context();
      track signer.id
    ) {
      <button
        tuiChip
        type="button"
        [appearance]="signer.status === 'trusted' ? 'positive' : 'neutral'"
        [iconStart]="signer.status === 'trusted' ? '@tui.check' : '@tui.minus'"
        (click.stop)="trust.viewSigner(signer)"
      >
        {{ signer.name || ('Unknown key' | i18n) }}
      </button>
    }
  `,
  styles: `
    :host {
      display: flex;
      flex-wrap: wrap;
      gap: 0.5rem;
    }
  `,
  imports: [SignerStatusesPipe, TuiChip, i18nPipe],
})
export class MarketplaceSignerChipsComponent {
  protected readonly trust = inject(MarketplaceTrust)

  readonly keys = input.required<readonly string[]>()
  readonly context = input<TrustContext>('marketplace')
}
