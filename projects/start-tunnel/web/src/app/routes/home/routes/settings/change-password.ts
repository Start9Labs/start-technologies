import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidatorFn,
  Validators,
} from '@angular/forms'
import { ErrorService } from '@start9labs/shared'
import { TuiAutoFocus, TuiValidator } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiIcon,
  TuiInput,
  TuiNotificationService,
  TuiTextfield,
} from '@taiga-ui/core'
import { TuiButtonLoading, TuiPassword } from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { map } from 'rxjs'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { ApiService } from 'src/app/services/api/api.service'

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form">
      <tui-textfield>
        <label tuiLabel>{{ 'New password' | i18n }}</label>
        <input
          tuiInput
          tuiAutoFocus
          type="password"
          autocomplete="new-password"
          formControlName="password"
        />
        <tui-icon tuiPassword />
      </tui-textfield>
      <tui-error formControlName="password" />
      <tui-textfield>
        <label tuiLabel>{{ 'Confirm new password' | i18n }}</label>
        <input
          tuiInput
          type="password"
          autocomplete="new-password"
          formControlName="confirm"
          [tuiValidator]="matchValidator()"
        />
        <tui-icon tuiPassword />
      </tui-textfield>
      <tui-error formControlName="confirm" />
      <footer>
        <button
          tuiButton
          (click)="onSave()"
          [loading]="loading()"
          [disabled]="formInvalid()"
        >
          {{ 'Save' | i18n }}
        </button>
      </footer>
    </form>
  `,
  hostDirectives: [ModalHelp],
  providers: [
    provideTranslatedValidationErrors({
      required: 'This field is required',
      minlength: 'Password must be at least 8 characters',
      maxlength: 'Password cannot exceed 64 characters',
      match: 'Passwords do not match',
    }),
    provideHelp('/settings/change-password'),
  ],
  imports: [
    ReactiveFormsModule,
    TuiAutoFocus,
    TuiButton,
    TuiButtonLoading,
    TuiError,
    TuiForm,
    TuiIcon,
    TuiInput,
    TuiPassword,
    TuiTextfield,
    TuiValidator,
    i18nPipe,
  ],
})
export class ChangePasswordDialog {
  private readonly context = injectContext<TuiDialogContext<void>>()
  private readonly api = inject(ApiService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly errorService = inject(ErrorService)
  private readonly i18n = inject(i18nPipe)

  protected readonly loading = signal(false)
  protected readonly form = inject(NonNullableFormBuilder).group({
    password: [
      '',
      [Validators.required, Validators.minLength(8), Validators.maxLength(64)],
    ],
    confirm: [
      '',
      [Validators.required, Validators.minLength(8), Validators.maxLength(64)],
    ],
  })

  protected readonly matchValidator = toSignal(
    this.form.controls.password.valueChanges.pipe(
      map(
        (password): ValidatorFn =>
          ({ value }) =>
            value === password ? null : { match: true },
      ),
    ),
    { initialValue: Validators.nullValidator },
  )

  protected readonly formInvalid = toSignal(
    this.form.statusChanges.pipe(map(() => this.form.invalid)),
    { initialValue: this.form.invalid },
  )

  protected async onSave() {
    this.loading.set(true)

    try {
      await this.api.setPassword({ password: this.form.getRawValue().password })
      this.alerts
        .open(this.i18n.transform('Password changed'), {
          label: this.i18n.transform('Success'),
          appearance: 'positive',
        })
        .subscribe()
      this.context.$implicit.complete()
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.loading.set(false)
    }
  }
}

export const CHANGE_PASSWORD = new PolymorpheusComponent(ChangePasswordDialog)
