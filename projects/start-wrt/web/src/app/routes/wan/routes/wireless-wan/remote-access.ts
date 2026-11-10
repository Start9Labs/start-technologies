import { Component } from '@angular/core'
import { TuiButton, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">{{ 'Remote Access is on' | i18n }}</h2>
    </header>
    <form
      tuiForm="m"
      (submit.prevent)="context.completeWith(true)"
      (reset)="context.completeWith(false)"
    >
      <p>
        {{
          'Devices on the network the router joins can reach its web interface and SSH while Remote Access is on.'
            | i18n
        }}
      </p>
      <footer>
        <button tuiButton type="reset" appearance="secondary">
          {{ 'Join anyway' | i18n }}
        </button>
        <button tuiButton>
          {{ 'Turn off Remote Access and join' | i18n }}
        </button>
      </footer>
    </form>
  `,
  imports: [TuiHeader, TuiTitle, TuiForm, TuiButton, i18nPipe],
})
class WwanRemoteAccessDialog {
  protected readonly context = injectContext<TuiDialogContext<boolean>>()
}

/** Emits whether to turn Remote Access off before joining. Completes empty on cancel. */
export const WWAN_REMOTE_ACCESS = new PolymorpheusComponent(
  WwanRemoteAccessDialog,
)
