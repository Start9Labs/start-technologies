import { Component, DestroyRef, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { i18nPipe, TaskService } from '@start9labs/shared'
import {
  TuiButton,
  TuiIcon,
  TuiInput,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiButtonLoading, TuiPassword } from '@taiga-ui/kit'
import { TuiCardLarge, TuiForm, TuiHeader } from '@taiga-ui/layout'

import { StartupApiService } from 'src/app/services/api/startup-api.service'
import { ConfigService } from 'src/app/services/config.service'
import { StateService } from 'src/app/services/state.service'

@Component({
  template: `
    <header tuiHeader>
      <h1 tuiTitle>{{ 'Unlock your server' | i18n }}</h1>
    </header>
    <p>
      {{
        'After every restart, your server and its services will remain offline until you unlock it with your password.'
          | i18n
      }}
    </p>
    @if (config.protocol === 'https:') {
      <form tuiForm="m" [formGroup]="form" (submit.prevent)="submit()">
        <tui-textfield>
          <label tuiLabel>{{ 'Password' | i18n }}</label>
          <input
            tuiInput
            type="password"
            autocomplete="current-password"
            formControlName="password"
            [readonly]="pending()"
          />
          <tui-icon tuiPassword />
        </tui-textfield>
        <footer>
          <button
            tuiButton
            [disabled]="pending() || formStatus() !== 'VALID'"
            [loading]="pending()"
          >
            {{ 'Unlock' | i18n }}
          </button>
        </footer>
      </form>
    } @else {
      <div tuiNotification appearance="warning" role="alert">
        {{
          'Open this server using its trusted HTTPS address before entering your password.'
            | i18n
        }}
      </div>
    }
  `,
  imports: [
    ReactiveFormsModule,
    TuiButton,
    TuiButtonLoading,
    TuiForm,
    TuiHeader,
    TuiIcon,
    TuiInput,
    TuiNotification,
    TuiPassword,
    TuiTitle,
    i18nPipe,
  ],
  hostDirectives: [TuiCardLarge],
})
export default class Unlock {
  private readonly api = inject(StartupApiService)
  private readonly tasks = inject(TaskService)
  private readonly state = inject(StateService)
  private readonly lifetime = new AbortController()

  protected readonly config = inject(ConfigService)
  protected readonly pending = signal(false)
  protected readonly form = inject(NonNullableFormBuilder).group({
    password: ['', Validators.required],
  })
  protected readonly formStatus = toSignal(this.form.statusChanges, {
    initialValue: this.form.status,
  })

  constructor() {
    inject(DestroyRef).onDestroy(() => {
      this.lifetime.abort()
      this.form.reset()
    })
  }

  protected async submit() {
    if (this.pending() || this.form.invalid) return
    this.pending.set(true)
    const { signal } = this.lifetime
    try {
      const completed = await this.tasks.runInline(async () => {
        try {
          const { items } = await this.api.streams.unlock(
            this.form.getRawValue(),
            { signal },
          )
          for await (const _item of items) {
          }
        } catch (error) {
          if (!signal.aborted) throw error
        }
      })
      if (completed && !signal.aborted) {
        this.form.reset()
        this.state.retrigger(true, 0)
      }
    } finally {
      this.pending.set(false)
    }
  }
}
