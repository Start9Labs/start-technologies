import { Component, inject } from '@angular/core'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidationErrors,
  Validators,
} from '@angular/forms'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiError,
  TuiIcon,
  TuiInput,
  TuiLabel,
  TuiTextfield,
  TuiTitle,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { TuiPassword } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import { ActionService } from 'src/app/services/action.service'
import { ApiService } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

function passwordsMatch(control: AbstractControl): ValidationErrors | null {
  const password = control.get('password')?.value
  const confirm = control.get('confirm')?.value
  if (password && confirm && password !== confirm) {
    control.get('confirm')?.setErrors({ mismatch: true })
    return { mismatch: true }
  }
  return null
}

@Component({
  template: `
    <form [formGroup]="form" [formLoading]="false" (ngSubmit)="onSubmit()">
      <header tuiHeader="body-l">
        <h2 tuiTitle>{{ 'Change Password' | i18n }}</h2>
      </header>
      <tui-textfield>
        <label tuiLabel>{{ 'Old password' | i18n }}</label>
        <input tuiInput formControlName="old" type="password" />
        <tui-icon tuiPassword />
      </tui-textfield>
      <tui-error formControlName="old" />
      <tui-textfield>
        <label tuiLabel>{{ 'New password' | i18n }}</label>
        <input tuiInput formControlName="password" type="password" />
        <tui-icon tuiPassword />
      </tui-textfield>
      <tui-error formControlName="password" />
      <tui-textfield>
        <label tuiLabel>{{ 'Confirm password' | i18n }}</label>
        <input tuiInput formControlName="confirm" type="password" />
        <tui-icon tuiPassword />
      </tui-textfield>
      <tui-error formControlName="confirm" />
      <footer appFooter></footer>
    </form>
  `,
  styles: `
    tui-textfield {
      max-inline-size: 20rem;
    }
  `,
  host: { class: 'g-page' },
  providers: [
    provideTranslatedValidationErrors({
      required: 'This field is required',
      minlength: 'Password must be at least 12 characters',
      mismatch: 'Passwords do not match',
    }),
  ],
  imports: [
    ReactiveFormsModule,
    TuiHeader,
    TuiTitle,
    TuiError,
    TuiIcon,
    TuiInput,
    TuiLabel,
    TuiPassword,
    TuiTextfield,
    Form,
    Footer,
    i18nPipe,
  ],
})
export default class Password {
  private readonly api = inject(ApiService)
  private readonly actions = inject(ActionService)
  private readonly i18n = inject(i18nPipe)

  public readonly form = inject(NonNullableFormBuilder).group(
    {
      old: ['', Validators.required],
      password: ['', [Validators.required, Validators.minLength(12)]],
      confirm: ['', Validators.required],
    },
    { validators: passwordsMatch },
  )

  async onSubmit(): Promise<void> {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)
    } else if (
      await this.actions.run(
        () =>
          this.api.setPassword({
            oldPassword: this.form.value.old!,
            newPassword: this.form.value.password!,
          }),
        { success: this.i18n.transform('Password changed successfully') },
      )
    ) {
      this.form.reset()
    }
  }
}
