import { CommonModule } from '@angular/common'
import { Component, inject, linkedSignal, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { Router } from '@angular/router'
import { AuthKeyService, i18nKey, i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiError,
  TuiIcon,
  TuiInput,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiButtonLoading, TuiPassword } from '@taiga-ui/kit'
import { TuiCardLarge, TuiForm, TuiHeader } from '@taiga-ui/layout'
import { CAWizardComponent } from 'src/app/routes/login/ca-wizard.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { AuthService } from 'src/app/services/auth.service'
import { ConfigService } from 'src/app/services/config.service'

@Component({
  selector: 'login',
  template: `
    @if (config.isSecureContext()) {
      <!-- Secure context -->
      <div tuiCardLarge>
        <img alt="StartOS Icon" src="assets/img/icon.png" />
        <header tuiHeader="h4">
          <h1 tuiTitle>{{ 'Login to StartOS' | i18n }}</h1>
        </header>
        <form tuiForm (submit)="submit()" [tuiTextfieldCleaner]="false">
          <tui-textfield iconStart="@tui.key">
            <label tuiLabel>{{ 'Password' | i18n }}</label>
            <input
              tuiInput
              type="password"
              [ngModelOptions]="{ standalone: true }"
              [(ngModel)]="password"
            />
            <tui-icon tuiPassword />
          </tui-textfield>
          <tui-error [error]="error() || authError() | i18n" />
          <button tuiButton size="m" [loading]="loading()">
            {{ 'Login' | i18n }}
          </button>
        </form>
      </div>
    } @else {
      <!-- Insecure context -->
      <ca-wizard />
    }
  `,
  styles: `
    :host {
      padding: 1rem;
      margin: auto;
      display: grid;
      place-items: center;
      max-width: max(70%, 40rem);
      min-height: 100dvh;

      [tuiTitle],
      [tuiButton] {
        min-width: 50%;
        border-radius: 10rem;
        margin: auto;
      }
    }

    [tuiCardLarge] {
      overflow: visible;
      width: max(33%, 22rem);
      background: var(--start9-base-1);
    }

    img {
      width: 6rem;
      margin: -5rem auto -0.5rem;
    }
  `,
  imports: [
    CommonModule,
    FormsModule,
    CAWizardComponent,
    TuiButton,
    TuiButtonLoading,
    TuiCardLarge,
    TuiInput,
    TuiIcon,
    TuiPassword,
    TuiError,
    i18nPipe,
    TuiHeader,
    TuiTitle,
    TuiForm,
  ],
})
export default class LoginPage {
  private readonly router = inject(Router)
  private readonly authService = inject(AuthService)
  private readonly authKeys = inject(AuthKeyService)
  private readonly api = inject(ApiService)

  protected readonly config = inject(ConfigService)
  protected readonly authError = this.authService.loginError

  readonly password = signal('')
  readonly loading = signal(false)
  readonly error = linkedSignal<string, i18nKey | null>({
    source: () => this.password(),
    computation: () => null,
  })

  async submit() {
    this.error.set(null)
    this.authService.loginError.set(null)
    this.loading.set(true)

    try {
      if (this.password().length > 64) {
        this.error.set('Password must be less than 65 characters')
        return
      }
      const key = await this.authKeys.create()
      try {
        await this.api.login({
          password: this.password(),
          pubkey: key.pubkeyPem,
          // kiosk mode re-enrolls on every browser restart; don't persist
          ephemeral: window.location.host === 'localhost',
        })
      } catch (e) {
        await this.authKeys.rollback()
        throw e
      }

      this.password.set('')
      this.authService.setVerified(true)
      // Await navigation so the button keeps loading until the route actually
      // changes — otherwise `finally` clears it the moment navigate() is called.
      await this.router.navigate([''], { replaceUrl: true })
    } catch (e: any) {
      // code 7 is for incorrect password
      this.error.set(e.code === 7 ? 'Invalid password' : (e.message as i18nKey))
    } finally {
      this.loading.set(false)
    }
  }
}
