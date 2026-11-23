import { Component, DestroyRef, inject, signal } from '@angular/core'
import { WA_NAVIGATOR } from '@ng-web-apis/common'
import { auth } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiIcon,
  TuiTextfield,
} from '@taiga-ui/core'
import { TuiButtonLoading, TuiCopy, TuiTextarea } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import { i18nPipe } from '../i18n/i18n.pipe'
import { i18nKey } from '../i18n/i18n.providers'
import { AuthKeyService } from '../services/auth-key.service'

export interface CliEnrollOptions {
  /** Enrolls the key that follows it. */
  readonly command: string
  /** Rejects until the server accepts this browser's key. */
  readonly probe: () => Promise<unknown>
}

@Component({
  template: `
    <p>
      {{
        'Sign in without a password by running this command in a terminal on the server, then select Continue.'
          | i18n
      }}
    </p>
    <tui-textfield>
      <textarea
        tuiTextarea
        [readonly]="true"
        [minRows]="7"
        [value]="command()"
      ></textarea>
      <tui-icon tuiCopy />
    </tui-textfield>
    <tui-error [error]="(error() | i18n) || null" />
    <footer class="g-buttons">
      <button
        tuiButton
        type="button"
        appearance="secondary"
        (click)="context.$implicit.complete()"
      >
        {{ 'Cancel' | i18n }}
      </button>
      <button
        tuiButton
        type="button"
        [disabled]="!command()"
        [loading]="checking()"
        (click)="continue()"
      >
        {{ 'Continue' | i18n }}
      </button>
    </footer>
  `,
  imports: [
    TuiButton,
    TuiButtonLoading,
    TuiCopy,
    TuiError,
    TuiIcon,
    TuiTextarea,
    TuiTextfield,
    i18nPipe,
  ],
})
export class CliEnrollDialog {
  private readonly authKeys = inject(AuthKeyService)
  private readonly userAgent = inject(WA_NAVIGATOR).userAgent
  private enrolled = false

  protected readonly context =
    injectContext<TuiDialogContext<boolean, CliEnrollOptions>>()
  protected readonly command = signal('')
  protected readonly checking = signal(false)
  protected readonly error = signal<i18nKey | null>(null)

  constructor() {
    const created = this.authKeys.create().then(key => {
      const der64 = auth.normalizeDer64(key.pubkeyPem)
      this.command.set(
        `${this.context.data.command} ${der64} --user-agent ${shellQuote(this.userAgent)}`,
      )
    })
    inject(DestroyRef).onDestroy(() => {
      if (!this.enrolled) created.then(() => this.authKeys.rollback())
    })
  }

  protected async continue() {
    this.checking.set(true)
    this.error.set(null)
    try {
      await this.context.data.probe()
      this.enrolled = true
      this.context.completeWith(true)
    } catch {
      this.error.set('This browser is not enrolled yet')
    } finally {
      this.checking.set(false)
    }
  }
}

export const CLI_ENROLL = new PolymorpheusComponent(CliEnrollDialog)

function shellQuote(value: string): string {
  return `'${value.replaceAll("'", `'\\''`)}'`
}
