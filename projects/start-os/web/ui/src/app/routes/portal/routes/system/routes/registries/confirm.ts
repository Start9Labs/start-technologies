import { KeyValuePipe } from '@angular/common'
import { Component } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiCell, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { KeyIdentity } from 'src/app/components/trusted-keys/identity'

@Component({
  template: `
    <form tuiForm="m" (submit.prevent)="context.completeWith(true)">
      <header tuiHeader>
        <h2 tuiTitle [id]="context.id">
          {{ 'Connect registry?' | i18n }}
          <span tuiSubtitle>
            {{ context.data.info.name || context.data.url }}
          </span>
          <span tuiSubtitle>{{ context.data.url }}</span>
        </h2>
      </header>
      @if (proposed.length) {
        <p>
          {{
            'This registry asks you to trust these keys for packages. Review them before connecting.'
              | i18n
          }}
        </p>
        <div>
          @for (key of context.data.info.trustKeys | keyvalue; track key.key) {
            <div tuiCell="m">
              <span
                [appKeyIdentity]="{
                  name: key.value,
                  publicKeys: [key.key],
                  gpgExports: [],
                }"
              ></span>
            </div>
          }
        </div>
      } @else {
        <p>
          {{ 'This registry is not requesting trust for any keys.' | i18n }}
        </p>
      }
      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>
          {{ (proposed.length ? 'Connect and trust' : 'Connect') | i18n }}
        </button>
      </footer>
    </form>
  `,
  imports: [
    KeyIdentity,
    KeyValuePipe,
    TuiButton,
    TuiCell,
    TuiForm,
    TuiHeader,
    TuiTitle,
    i18nPipe,
  ],
})
class ConfirmRegistry {
  protected readonly context =
    injectContext<
      TuiDialogContext<boolean, { url: string; info: T.RegistryInfo }>
    >()
  protected readonly proposed = Object.keys(this.context.data.info.trustKeys)
}

export const CONFIRM_REGISTRY = new PolymorpheusComponent(ConfirmRegistry)
