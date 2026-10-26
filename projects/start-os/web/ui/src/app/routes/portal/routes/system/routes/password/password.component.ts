import { Component, inject, viewChild } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import {
  ErrorService,
  i18nKey,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { ISB } from '@start9labs/start-core'
import { TuiButton, TuiNotificationService, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { from } from 'rxjs'
import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { TitleDirective } from 'src/app/services/title.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'

@Component({
  template: `
    <ng-container *title>
      <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
        {{ 'Back' | i18n }}
      </a>
      {{ 'Change Password' | i18n }}
    </ng-container>
    <header tuiHeader>
      <hgroup tuiTitle>
        <h3>{{ 'Change Password' | i18n }}</h3>
        <p tuiSubtitle>
          {{
            'You will still need your current password to decrypt existing backups!'
              | i18n
          }}
        </p>
      </hgroup>
    </header>
    @if (spec(); as spec) {
      <app-form [spec]="spec" [buttons]="buttons" />
    }
  `,
  styles: `
    :host {
      max-inline-size: 40rem;
    }

    section {
      padding: 4rem 1rem 0;
    }
  `,
  imports: [
    RouterLink,
    TuiHeader,
    TuiTitle,
    TuiButton,
    FormComponent,
    TitleDirective,
    i18nPipe,
  ],
})
export default class SystemPasswordComponent {
  private readonly alerts = inject(TuiNotificationService)
  private readonly tasks = inject(TaskService)
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  readonly form = viewChild(FormComponent)
  readonly spec = toSignal(from(configBuilderToSpec(this.passwordSpec())))
  readonly buttons = [
    {
      text: this.i18n.transform('Save')!,
      handler: (value: ReturnType<typeof this.passwordSpec>['_TYPE']) =>
        this.resetPassword(value),
    },
  ]

  private resetPassword({
    newPassword,
    newPasswordConfirm,
  }: ReturnType<typeof this.passwordSpec>['_TYPE']) {
    let error: i18nKey | null = null

    if (newPassword !== newPasswordConfirm) {
      error = 'New passwords do not match'
    } else if (newPassword.length < 12) {
      error = 'New password must be 12 characters or greater'
    } else if (newPassword.length > 64) {
      error = 'New password must be less than 65 characters'
    }

    if (error) {
      this.errorService.handleError(error)
      return
    }

    this.tasks.run(async () => {
      await this.api.rpc.auth.resetPassword({ newPassword })
      this.form()?.form.reset()
      this.alerts
        .open(this.i18n.transform('Password changed'), {
          appearance: 'positive',
        })
        .subscribe()
    }, 'Saving')
  }

  passwordSpec() {
    return ISB.InputSpec.of({
      newPassword: ISB.Value.text({
        name: this.i18n.transform('New Password')!,
        required: true,
        default: null,
        masked: true,
      }),
      newPasswordConfirm: ISB.Value.text({
        name: this.i18n.transform('Retype New Password')!,
        required: true,
        default: null,
        masked: true,
      }),
    })
  }
}
