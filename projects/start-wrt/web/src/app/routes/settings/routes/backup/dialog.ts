import { Component, inject } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiIcon,
  TuiInput,
  TuiTextfield,
} from '@taiga-ui/core'
import { TuiPassword } from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'

interface BackupPasswordData {
  message: string
  action: string
}

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="submit()">
      <p>{{ context.data.message }}</p>
      <tui-textfield>
        <label tuiLabel>{{ 'Password' | i18n }}</label>
        <input tuiInput formControlName="password" type="password" />
        <tui-icon tuiPassword />
      </tui-textfield>
      <tui-error formControlName="password" />
      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>{{ context.data.action }}</button>
      </footer>
    </form>
  `,
  providers: [
    provideTranslatedValidationErrors({ required: 'This field is required' }),
  ],
  imports: [
    ReactiveFormsModule,
    TuiForm,
    TuiTextfield,
    TuiInput,
    TuiIcon,
    TuiPassword,
    TuiError,
    TuiButton,
    i18nPipe,
  ],
})
class BackupPassword {
  protected readonly context =
    injectContext<TuiDialogContext<string, BackupPasswordData>>()

  protected readonly form = inject(NonNullableFormBuilder).group({
    password: ['', Validators.required],
  })

  protected submit(): void {
    tuiMarkControlAsTouchedAndValidate(this.form)

    if (this.form.valid) {
      this.context.completeWith(this.form.getRawValue().password)
    }
  }
}

export const BACKUP_PASSWORD = new PolymorpheusComponent(BackupPassword)
