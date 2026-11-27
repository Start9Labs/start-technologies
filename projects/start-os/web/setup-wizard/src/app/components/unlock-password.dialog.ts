import { Component, linkedSignal, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiDialogContext, TuiError, TuiInput } from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

@Component({
  imports: [FormsModule, TuiButton, TuiError, TuiInput, i18nPipe],
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2 [id]="context.id">{{ 'Unlock backup' | i18n }}</h2>
        <p>
          {{
            'Enter the password that was used to encrypt this backup.' | i18n
          }}
        </p>
      </hgroup>
    </header>
    <tui-textfield>
      <label tuiLabel>{{ 'Password' | i18n }}</label>
      <input
        tuiInput
        autocapitalize="off"
        autocomplete="current-password"
        [type]="passwordMasked() ? 'password' : 'text'"
        [(ngModel)]="password"
        (keyup.enter)="unlock()"
      />
      <button
        tuiIconButton
        type="button"
        size="xs"
        appearance="icon"
        [attr.aria-label]="
          (passwordMasked() ? 'Show password' : 'Hide password') | i18n
        "
        [iconStart]="passwordMasked() ? '@tui.eye' : '@tui.eye-off'"
        (click)="passwordMasked.set(!passwordMasked())"
      >
        {{ (passwordMasked() ? 'Show password' : 'Hide password') | i18n }}
      </button>
    </tui-textfield>
    <tui-error [error]="error() ? ('Required' | i18n) : null" />
    <footer>
      <button tuiButton appearance="flat" (click)="context.completeWith(null)">
        {{ 'Cancel' | i18n }}
      </button>
      <button tuiButton (click)="unlock()">
        {{ 'Unlock' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    footer {
      display: flex;
      justify-content: flex-end;
      gap: 0.5rem;
      margin-block-start: 1.5rem;
    }
  `,
})
export class UnlockPasswordDialog {
  protected readonly context = injectContext<TuiDialogContext<string | null>>()

  protected readonly password = signal('')
  protected readonly passwordMasked = signal(true)
  protected readonly error = linkedSignal({
    source: this.password,
    computation: () => false,
  })

  protected unlock() {
    if (!this.password()) {
      this.error.set(true)
      return
    }
    this.context.completeWith(this.password())
  }
}

export const UNLOCK_PASSWORD = new PolymorpheusComponent(UnlockPasswordDialog)
