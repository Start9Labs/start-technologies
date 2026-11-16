import { Component, inject, input } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { TuiChip } from '@taiga-ui/kit'
import {
  MarketplaceTrust,
  SIGNER_STATUS_LABELS,
  SignerStatusesPipe,
  TrustContext,
} from '../trust'

@Component({
  selector: 'marketplace-signer-chips',
  template: `
    @for (
      signer of keys() | signerStatuses: trust.keys() ?? {} : context();
      track signer.id
    ) {
      @let name = signer.name || ('Unknown key' | i18n);
      @let status = statusLabels[signer.status] | i18n;
      <button
        tuiChip
        type="button"
        [appearance]="signer.status === 'trusted' ? 'positive' : 'neutral'"
        [iconStart]="signer.status === 'trusted' ? '@tui.check' : '@tui.minus'"
        [attr.aria-label]="name === status ? name : name + ': ' + status"
        (click.stop)="trust.viewSigner(signer)"
      >
        {{ name }}
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
  protected readonly statusLabels = SIGNER_STATUS_LABELS

  readonly keys = input.required<readonly string[]>()
  readonly context = input<TrustContext>('marketplace')
}
