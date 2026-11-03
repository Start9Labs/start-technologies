import { Component, inject, input } from '@angular/core'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import { TuiCell, TuiCellStretch, TuiIcon, TuiTitle } from '@taiga-ui/core'
import {
  MarketplaceTrust,
  SignerStatus,
  SignerStatusesPipe,
  TrustContext,
} from '../trust'

const STATUSES: Record<
  SignerStatus,
  { icon: string; class: string; label: i18nKey }
> = {
  trusted: { icon: '@tui.check', class: 'g-positive', label: 'Trusted' },
  untrusted: { icon: '@tui.x', class: 'g-negative', label: 'Not trusted' },
  unknown: { icon: '@tui.minus', class: 'g-secondary', label: 'Unknown key' },
}

@Component({
  selector: 'marketplace-signers',
  template: `
    @for (
      signer of keys() | signerStatuses: trust.keys() ?? {} : context();
      track signer.id
    ) {
      <button
        tuiCell
        tuiCellStretch
        type="button"
        (click.stop)="trust.viewSigner(signer)"
      >
        <tui-icon
          [icon]="statuses[signer.status].icon"
          [class]="statuses[signer.status].class"
          [attr.aria-label]="statuses[signer.status].label | i18n"
        />
        <span tuiTitle>
          {{ signer.name || ('Unknown key' | i18n) }}
          @for (email of signer.emails; track email) {
            <span tuiSubtitle>{{ email }}</span>
          }
          @if (signer.status === 'unknown') {
            @for (fingerprint of signer.fingerprints; track fingerprint) {
              <span tuiSubtitle class="fingerprint">{{ fingerprint }}</span>
            }
          }
        </span>
        <tui-icon icon="@tui.chevron-right" />
      </button>
    }
  `,
  styles: `
    :host {
      display: contents;
    }

    .fingerprint {
      overflow-wrap: anywhere;
    }
  `,
  imports: [
    SignerStatusesPipe,
    TuiCell,
    TuiCellStretch,
    TuiIcon,
    TuiTitle,
    i18nPipe,
  ],
})
export class MarketplaceSignersComponent {
  protected readonly trust = inject(MarketplaceTrust)
  protected readonly statuses = STATUSES

  readonly keys = input.required<readonly string[]>()
  readonly context = input<TrustContext>('marketplace')
}
