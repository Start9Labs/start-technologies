import { Component, inject, signal } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiInput,
  TuiTitle,
  tuiValidationErrorsProvider,
} from '@taiga-ui/core'
import { TuiButtonLoading, TuiSwitch } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { ApiService } from 'src/app/services/api/embassy-api.service'

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <header tuiHeader>
        <h2 tuiTitle [id]="context.id">{{ 'Edit trusted keyring' | i18n }}</h2>
      </header>
      <tui-textfield>
        <label tuiLabel>{{ 'Name' | i18n }}</label>
        <input tuiInput formControlName="name" />
      </tui-textfield>
      <tui-error formControlName="name" />
      <fieldset>
        <legend>{{ 'Trust signatures for:' | i18n }}</legend>
        <label tuiLabel>
          <input tuiSwitch type="checkbox" formControlName="marketplace" />
          {{ 'Service Packages' | i18n }}
        </label>
        <label tuiLabel>
          <input tuiSwitch type="checkbox" formControlName="os" />
          {{ 'OS Updates' | i18n }}
        </label>
      </fieldset>
      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton [loading]="saving()">{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  styles: `
    fieldset > label {
      grid-column: 1 / -1;
    }
  `,
  imports: [
    ReactiveFormsModule,
    TuiButton,
    TuiButtonLoading,
    TuiError,
    TuiForm,
    TuiHeader,
    TuiInput,
    TuiSwitch,
    TuiTitle,
    i18nPipe,
  ],
  providers: [
    tuiValidationErrorsProvider(() => {
      const i18n = inject(i18nPipe)
      return {
        required: i18n.transform('Required'),
        pattern: i18n.transform('Required'),
      }
    }),
  ],
})
export class EditTrustedKeyDialog {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly context =
    injectContext<
      TuiDialogContext<void, { id: string; keyring: T.TrustedKeyring }>
    >()
  protected readonly saving = signal(false)
  protected readonly form = inject(NonNullableFormBuilder).group({
    name: [
      {
        value: this.context.data.keyring.name,
        disabled: !this.context.data.keyring.removable,
      },
      [Validators.required, Validators.pattern(/\S/)],
    ],
    marketplace: [this.context.data.keyring.marketplace],
    os: [this.context.data.keyring.os],
  })

  protected async save(): Promise<void> {
    tuiMarkControlAsTouchedAndValidate(this.form)
    if (this.form.invalid || this.saving()) return
    this.saving.set(true)
    const saved = await this.tasks.run(async () => {
      await this.api.rpc.server.trustedKeys.update({
        id: this.context.data.id,
        ...this.form.getRawValue(),
      })
    }, 'Saving')
    this.saving.set(false)
    if (saved) this.context.$implicit.complete()
  }
}

export const EDIT_TRUSTED_KEY = new PolymorpheusComponent(EditTrustedKeyDialog)
