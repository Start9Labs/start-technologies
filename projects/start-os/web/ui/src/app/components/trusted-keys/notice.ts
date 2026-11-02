import { Component, inject } from '@angular/core'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiCell, TuiDialogContext } from '@taiga-ui/core'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'
import { KeyIdentity } from './identity'

interface Notice {
  message: i18nKey
  ids: readonly string[]
  yes: i18nKey
  no?: i18nKey
}

@Component({
  template: `
    <form tuiForm="m" (submit.prevent)="context.completeWith(true)">
      <p>{{ context.data.message | i18n }}</p>
      @if (trust.keys(); as keys) {
        <div>
          @for (id of context.data.ids; track id) {
            @if (keys[id]; as key) {
              <div tuiCell="m"><span [appKeyIdentity]="key"></span></div>
            }
          }
        </div>
      }
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
        <button tuiButton>{{ context.data.yes | i18n }}</button>
      </footer>
    </form>
  `,
  imports: [KeyIdentity, TuiButton, TuiCell, TuiForm, i18nPipe],
})
class TrustedKeysNotice {
  protected readonly context =
    injectContext<TuiDialogContext<boolean, Notice>>()
  protected readonly trust = inject(TrustedKeysService)
}

export const TRUSTED_KEYS_NOTICE = new PolymorpheusComponent(TrustedKeysNotice)
