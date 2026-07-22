import { Component, inject, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { Router } from '@angular/router'
import { AuthKeyService } from '@start9labs/shared'
import { TuiButton, TuiError, TuiInput } from '@taiga-ui/core'
import { TuiButtonLoading } from '@taiga-ui/kit'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { i18nKey } from 'src/app/i18n/i18n.providers'
import { ApiService } from 'src/app/services/api/api.service'
import { AuthService } from 'src/app/services/auth.service'

@Component({
  template: `
    <img alt="Start9" src="assets/icons/favicon.svg" />
    <form (ngSubmit)="login()">
      <tui-textfield [tuiTextfieldCleaner]="false">
        <input
          tuiInput
          type="password"
          [placeholder]="'Enter password' | i18n"
          [ngModelOptions]="{ standalone: true }"
          [(ngModel)]="password"
          (ngModelChange)="error.set(null)"
          [disabled]="loading()"
        />
        <button
          tuiIconButton
          appearance="action"
          iconStart="@tui.log-in"
          [loading]="loading()"
        >
          {{ 'Login' | i18n }}
        </button>
      </tui-textfield>
      @if (error(); as err) {
        <tui-error [error]="err | i18n" />
      }
    </form>
  `,
  styles: `
    :host {
      height: 100%;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 2rem;
    }

    img {
      width: 5rem;
      height: 5rem;
    }

    tui-textfield {
      width: 18rem;
    }
  `,
  imports: [
    TuiButton,
    TuiInput,
    FormsModule,
    TuiError,
    TuiButtonLoading,
    i18nPipe,
  ],
})
export default class Login {
  private readonly auth = inject(AuthService)
  private readonly router = inject(Router)
  private readonly authKeys = inject(AuthKeyService)
  private readonly api = inject(ApiService)

  protected readonly error = signal<i18nKey | null>(null)
  protected readonly loading = signal(false)

  password = ''

  protected async login() {
    this.loading.set(true)
    try {
      const key = await this.authKeys.create()
      try {
        await this.api.login({
          password: this.password,
          pubkey: key.pubkeyPem,
          ephemeral: false,
        })
      } catch (e) {
        await this.authKeys.rollback()
        throw e
      }
      this.auth.authenticated.set(true)
      this.router.navigate(['.'])
    } catch (e: any) {
      // Code 7 is a wrong password; anything else (unrecognized server
      // identity, clock skew, rate limit) carries its own message.
      this.error.set(e.code === 7 ? 'Password is invalid' : e.message)
    } finally {
      this.loading.set(false)
    }
  }
}
