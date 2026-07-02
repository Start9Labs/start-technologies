import { Component, inject, signal } from '@angular/core'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidationErrors,
  Validators,
} from '@angular/forms'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiInput,
  TuiTextfield,
  tuiTextfieldOptionsProvider,
} from '@taiga-ui/core'
import { TuiTextarea } from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'

function publicKeyValidator(control: AbstractControl): ValidationErrors | null {
  const value = control.value?.trim()
  if (!value) return null

  const parts = value.split(/\s+/)
  if (parts.length < 2) return { invalidKey: true }

  const validAlgorithms =
    /^(ssh-(ed25519|rsa|dss)|ecdsa-sha2-nistp(256|384|521))$/
  if (!validAlgorithms.test(parts[0])) return { invalidKey: true }

  const base64 = /^[A-Za-z0-9+/]+=*$/
  if (!base64.test(parts[1])) return { invalidKey: true }

  return null
}

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <tui-textfield>
        <label tuiLabel>{{ 'Public Key' | i18n }}</label>
        <textarea
          tuiTextarea
          placeholder="ssh-ed25519 AAAA... user@host"
          formControlName="key"
          [min]="3"
        ></textarea>
      </tui-textfield>
      <tui-error formControlName="key" />
      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>{{ 'Add Key' | i18n }}</button>
      </footer>
    </form>
  `,
  hostDirectives: [ModalHelp],
  providers: [
    provideHelp('/settings/ssh-keys/dialog'),
    tuiTextfieldOptionsProvider({ cleaner: signal(false) }),
    provideTranslatedValidationErrors({
      invalidKey: 'Invalid public key format',
    }),
  ],
  imports: [
    ReactiveFormsModule,
    TuiForm,
    TuiTextfield,
    TuiError,
    TuiInput,
    TuiButton,
    TuiTextarea,
    i18nPipe,
  ],
})
export class AddSshKey {
  protected readonly context = injectContext<TuiDialogContext<string, void>>()
  protected readonly form = inject(NonNullableFormBuilder).group({
    key: ['', [Validators.required, publicKeyValidator]],
  })

  protected save(): void {
    tuiMarkControlAsTouchedAndValidate(this.form)

    if (this.form.valid) {
      this.context.completeWith(this.form.value.key!)
    }
  }
}

export const ADD_SSH_KEY = new PolymorpheusComponent(AddSshKey)
