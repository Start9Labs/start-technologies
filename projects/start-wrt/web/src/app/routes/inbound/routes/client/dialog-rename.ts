import { Component, inject } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import {
  TuiButton,
  TuiDialogContext,
  TuiInput,
  TuiTextfield,
} from '@taiga-ui/core'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <tui-textfield>
        <label tuiLabel>{{ 'Name' | i18n }}</label>
        <input tuiInput formControlName="name" />
      </tui-textfield>
      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  hostDirectives: [ModalHelp],
  providers: [provideHelp('/inbound/client/dialog-rename')],
  imports: [
    ReactiveFormsModule,
    TuiForm,
    TuiTextfield,
    TuiInput,
    TuiButton,
    i18nPipe,
  ],
})
class RenameClient {
  protected readonly context = injectContext<TuiDialogContext<string, string>>()

  protected readonly form = inject(NonNullableFormBuilder).group({
    name: [this.context.data, Validators.required],
  })

  protected save(): void {
    if (this.form.valid) {
      this.context.completeWith(this.form.value.name!)
    }
  }
}

export const RENAME_CLIENT = new PolymorpheusComponent(RenameClient)
