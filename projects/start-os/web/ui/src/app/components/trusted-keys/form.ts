import { Component, inject, signal } from '@angular/core'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiInput,
  TuiTitle,
  tuiValidationErrorsProvider,
} from '@taiga-ui/core'
import { TuiButtonLoading, TuiTextarea } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { parsePublicKeys } from 'src/app/utils/public-keys'

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <header tuiHeader>
        <h2 tuiTitle [id]="context.id">{{ 'Add trusted keyring' | i18n }}</h2>
      </header>
      <tui-textfield>
        <label tuiLabel>{{ 'Name' | i18n }}</label>
        <input tuiInput formControlName="name" />
      </tui-textfield>
      <tui-error formControlName="name" />
      <tui-textfield>
        <label tuiLabel>{{ 'Public keys' | i18n }}</label>
        <textarea
          tuiTextarea
          formControlName="publicKeys"
          [min]="5"
          [max]="12"
          spellcheck="false"
          autocapitalize="off"
        ></textarea>
      </tui-textfield>
      <tui-error formControlName="publicKeys" />
      <p>
        {{
          'Enter one or more Ed25519 public keys in PEM or GPG format.' | i18n
        }}
      </p>
      <footer>
        <button
          tuiButton
          type="button"
          appearance="secondary"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton [loading]="saving()">{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  imports: [
    ReactiveFormsModule,
    TuiButton,
    TuiButtonLoading,
    TuiError,
    TuiForm,
    TuiHeader,
    TuiInput,
    TuiTextarea,
    TuiTitle,
    i18nPipe,
  ],
  providers: [
    tuiValidationErrorsProvider(() => {
      const i18n = inject(i18nPipe)
      return {
        required: i18n.transform('Required'),
        pattern: i18n.transform('Required'),
        publicKeys: i18n.transform(
          'Enter one or more Ed25519 public keys in PEM or GPG format.',
        ),
      }
    }),
  ],
})
export class AddTrustedKeyDialog {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly context = injectContext<TuiDialogContext<void>>()
  protected readonly saving = signal(false)
  protected readonly form = inject(NonNullableFormBuilder).group({
    name: ['', [Validators.required, Validators.pattern(/\S/)]],
    publicKeys: [
      '',
      (control: AbstractControl<string>) => {
        try {
          parsePublicKeys(control.value)
          return null
        } catch {
          return { publicKeys: true }
        }
      },
    ],
  })

  protected async save(): Promise<void> {
    tuiMarkControlAsTouchedAndValidate(this.form)
    if (this.form.invalid || this.saving()) return
    this.saving.set(true)
    const { name, publicKeys } = this.form.getRawValue()
    const saved = await this.tasks.run(async () => {
      await this.api.rpc.server.trustedKeys.add({
        name,
        publicKeys: parsePublicKeys(publicKeys),
        context: 'marketplace',
      })
    }, 'Saving')
    this.saving.set(false)
    if (saved) this.context.$implicit.complete()
  }
}

export const ADD_TRUSTED_KEY = new PolymorpheusComponent(AddTrustedKeyDialog)
