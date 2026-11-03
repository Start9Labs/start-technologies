import { Component } from '@angular/core'
import { MarketplaceSignersComponent } from '@start9labs/marketplace'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiDialogContext } from '@taiga-ui/core'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

interface Notice {
  message: i18nKey
  ids: readonly string[]
  yes?: i18nKey
  no?: i18nKey
}

@Component({
  template: `
    <form tuiForm="m" (submit.prevent)="context.completeWith(true)">
      <p>{{ context.data.message | i18n }}</p>
      <div class="keys">
        <marketplace-signers [keys]="context.data.ids" />
      </div>
      @if (context.data.yes; as yes) {
        <footer>
          @if (context.data.no; as no) {
            <button
              tuiButton
              type="button"
              appearance="flat"
              (click)="context.completeWith(false)"
            >
              {{ no | i18n }}
            </button>
          }
          <button tuiButton>{{ yes | i18n }}</button>
        </footer>
      }
    </form>
  `,
  styles: `
    .keys {
      --tui-padding-inline: 0.5rem;
    }
  `,
  imports: [MarketplaceSignersComponent, TuiButton, TuiForm, i18nPipe],
})
class TrustedKeysNotice {
  protected readonly context =
    injectContext<TuiDialogContext<boolean, Notice>>()
}

export const TRUSTED_KEYS_NOTICE = new PolymorpheusComponent(TrustedKeysNotice)
