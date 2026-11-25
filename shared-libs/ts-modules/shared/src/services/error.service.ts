import { Component, ErrorHandler, inject, Service } from '@angular/core'
import { TuiNotificationOptions, TuiNotificationService } from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import { i18nPipe } from '../i18n/i18n.pipe'
import { getErrorMessage } from '../util/error-message'

interface ErrorToast {
  readonly message: string
  readonly link?: string
}

@Component({
  template: `
    {{ context.data.message }}
    @if (context.data.link; as link) {
      <a target="_blank" rel="noreferrer" [href]="link">
        {{ 'Get help' | i18n }}
      </a>
    }
  `,
  styles: `
    :host {
      white-space: pre-line;
    }

    a {
      display: block;
      margin-block-start: 1rem;
    }
  `,
  imports: [i18nPipe],
})
class ErrorToastComponent {
  protected readonly context =
    injectContext<TuiNotificationOptions<ErrorToast>>()
}

const ERROR_TOAST = new PolymorpheusComponent(ErrorToastComponent)

@Service()
export class ErrorService extends ErrorHandler {
  private readonly alerts = inject(TuiNotificationService)

  override handleError(error: unknown, link?: string) {
    console.error(error)

    this.alerts
      .open(ERROR_TOAST, {
        label: 'Error',
        appearance: 'negative',
        data: { message: getErrorMessage(error), link },
      })
      .subscribe()
  }
}
