import { Component, inject, signal } from '@angular/core'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidatorFn,
} from '@angular/forms'
import { i18nPipe, TaskService } from '@start9labs/shared'
import {
  tuiMarkControlAsTouchedAndValidate,
  TuiMapperPipe,
  TuiValidator,
} from '@taiga-ui/cdk'
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
import { T } from '@start9labs/start-core'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { parsePublicKeys } from 'src/app/utils/public-keys'

const PEM_KEY = /-----BEGIN PUBLIC KEY-----/

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <header tuiHeader>
        <h2 tuiTitle [id]="context.id">{{ 'Add key' | i18n }}</h2>
      </header>
      <tui-textfield>
        <label tuiLabel>{{ 'Public key' | i18n }}</label>
        <textarea
          tuiTextarea
          formControlName="publicKeys"
          [placeholder]="
            'Enter an Ed25519 public key in PEM or GPG format.' | i18n
          "
          [min]="5"
          [max]="12"
          spellcheck="false"
          autocapitalize="off"
        ></textarea>
      </tui-textfield>
      <tui-error formControlName="publicKeys" />
      @if (form.controls.publicKeys.value | tuiMapper: isPem) {
        <tui-textfield>
          <label tuiLabel>{{ 'Label' | i18n }}</label>
          <input tuiInput formControlName="name" [tuiValidator]="required" />
        </tui-textfield>
        <tui-error formControlName="name" />
      }
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
    TuiMapperPipe,
    TuiTextarea,
    TuiTitle,
    TuiValidator,
    i18nPipe,
  ],
  providers: [
    tuiValidationErrorsProvider(() => {
      const i18n = inject(i18nPipe)
      return {
        required: i18n.transform('Required'),
        publicKeys: i18n.transform(
          'Enter an Ed25519 public key in PEM or GPG format.',
        ),
      }
    }),
  ],
})
export class AddTrustedKeyDialog {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly context =
    injectContext<TuiDialogContext<T.AddTrustedKeyRes>>()
  protected readonly saving = signal(false)
  protected readonly form = inject(NonNullableFormBuilder).group({
    name: [''],
    publicKeys: [
      '',
      (control: AbstractControl<string>) => {
        try {
          return parsePublicKeys(control.value).length === 1
            ? null
            : { publicKeys: true }
        } catch {
          return { publicKeys: true }
        }
      },
    ],
  })

  protected readonly isPem = (publicKeys: string) => PEM_KEY.test(publicKeys)
  protected readonly required: ValidatorFn = ({ value }) =>
    value.trim() ? null : { required: true }

  protected async save(): Promise<void> {
    tuiMarkControlAsTouchedAndValidate(this.form)
    if (this.form.invalid || this.saving()) return
    this.saving.set(true)
    const { name, publicKeys } = this.form.getRawValue()
    await this.tasks.run(async () => {
      this.context.completeWith(
        await this.api.rpc.server.trustedKeys.add({
          name: this.isPem(publicKeys) ? name : '',
          publicKeys: parsePublicKeys(publicKeys),
        }),
      )
    }, 'Saving')
    this.saving.set(false)
  }
}

export const ADD_TRUSTED_KEY = new PolymorpheusComponent(AddTrustedKeyDialog)
