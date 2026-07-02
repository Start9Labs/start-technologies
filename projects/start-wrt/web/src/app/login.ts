import {
  Component,
  computed,
  DOCUMENT,
  inject,
  linkedSignal,
  signal,
} from '@angular/core'
import { FormsModule } from '@angular/forms'
import { Router } from '@angular/router'
import { TuiButton, TuiError, TuiInput, TuiTextfield } from '@taiga-ui/core'
import { TuiButtonLoading } from '@taiga-ui/kit'
import { ApiService } from 'src/app/services/api/api.service'
import { AuthService } from 'src/app/services/auth.service'
import { CaWizard } from 'src/app/components/ca-wizard'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    @if (showWizard()) {
      <app-ca-wizard />
    } @else {
      <img alt="Start9" src="assets/favicon.svg" />
      <form (ngSubmit)="login()">
        <tui-textfield>
          <input
            tuiInput
            type="password"
            [placeholder]="'Enter password' | i18n"
            [disabled]="loading()"
            [ngModelOptions]="{ standalone: true }"
            [(ngModel)]="password"
          />
          <button
            tuiIconButton
            appearance="action"
            iconStart="@tui.log-in"
            [disabled]="!password().length"
            [loading]="loading()"
          >
            {{ 'Login' | i18n }}
          </button>
        </tui-textfield>
        <tui-error [error]="error() ? ('Password is invalid' | i18n) : null" />
      </form>
    }
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

    :host-context(body:not([tuiTheme])) {
      img {
        filter: invert(1);
      }
    }
  `,
  imports: [
    FormsModule,
    TuiButton,
    TuiTextfield,
    TuiError,
    TuiButtonLoading,
    TuiInput,
    CaWizard,
    i18nPipe,
  ],
})
export default class Login {
  private readonly auth = inject(AuthService)
  private readonly router = inject(Router)
  private readonly api = inject(ApiService)
  private readonly document = inject(DOCUMENT)

  private readonly isLanHttp = computed(() => {
    const loc = this.document.location
    return loc.protocol === 'http:' && loc.hostname !== 'localhost'
  })

  protected readonly showWizard = this.isLanHttp

  protected readonly password = signal('')
  protected readonly loading = signal(false)
  protected readonly error = linkedSignal<boolean>(
    () => !!this.password() && false,
  )

  protected async login() {
    this.loading.set(true)
    try {
      await this.api.login({ password: this.password() })
      this.auth.authenticated.set(true)
      this.router.navigate(['.'])
    } catch (e) {
      this.error.set(true)
    } finally {
      this.loading.set(false)
    }
  }
}
