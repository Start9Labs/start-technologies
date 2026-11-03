import { Component, inject, input, Pipe } from '@angular/core'
import { DialogService, i18nPipe } from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'
import { MarketplaceTrust, Signer, SignerStatusesPipe } from '../trust'
import { MarketplaceSignersComponent } from './signers.component'

@Pipe({ name: 'signerPreviewKeys' })
class SignerPreviewKeys {
  transform(signers: readonly Signer[]): string[] {
    return signers
      .filter(signer => signer.status === signers[0]?.status)
      .slice(0, 3)
      .map(signer => signer.id)
  }
}

@Component({
  selector: 'marketplace-signer-preview',
  template: `
    @let signers = keys() | signerStatuses: trust.keys() ?? {};
    @let preview = signers | signerPreviewKeys;
    <marketplace-signers [keys]="preview" />
    @if (preview.length < signers.length) {
      <button
        tuiButton
        type="button"
        appearance="flat-grayscale"
        size="s"
        (click.stop)="
          dialog
            .openComponent(all, { label: 'Signed by', size: 's' })
            .subscribe()
        "
      >
        {{ 'View all' | i18n }}
      </button>
    }
    <ng-template #all>
      <marketplace-signers [keys]="keys()" />
    </ng-template>
  `,
  styles: `
    :host {
      display: contents;
    }
  `,
  imports: [
    MarketplaceSignersComponent,
    SignerPreviewKeys,
    SignerStatusesPipe,
    TuiButton,
    i18nPipe,
  ],
})
export class MarketplaceSignerPreview {
  protected readonly dialog = inject(DialogService)
  protected readonly trust = inject(MarketplaceTrust)

  public readonly keys = input.required<readonly string[]>()
}
