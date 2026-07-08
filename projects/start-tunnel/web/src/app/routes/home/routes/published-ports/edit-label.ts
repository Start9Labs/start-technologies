import { Component, inject } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext, TuiError, TuiInput } from '@taiga-ui/core'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { ApiService } from 'src/app/services/api/api.service'

export interface EditLabelData {
  readonly source: string
  readonly label: T.Tunnel.SniRoute['label']
  readonly hostname: string | null
  // Set for a v6 pinhole row — relabel the pinhole instead of a v4 forward.
  readonly pinhole?: { readonly gua: string; readonly externalPort: number }
}

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form">
      <tui-textfield>
        <label tuiLabel>{{ 'Label' | i18n }}</label>
        <input tuiInput formControlName="label" />
      </tui-textfield>
      <tui-error formControlName="label" />
      <footer>
        <button tuiButton [disabled]="form.invalid" (click)="onSave()">
          {{ 'Save' | i18n }}
        </button>
      </footer>
    </form>
  `,
  imports: [
    ReactiveFormsModule,
    TuiButton,
    TuiError,
    TuiInput,
    TuiForm,
    i18nPipe,
  ],
  hostDirectives: [ModalHelp],
  providers: [provideHelp('/published-ports/edit-label')],
})
export class PublishedPortsEditLabel {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly context =
    injectContext<TuiDialogContext<void, EditLabelData>>()

  protected readonly form = inject(NonNullableFormBuilder).group({
    label: [this.context.data.label, Validators.required],
  })

  protected async onSave() {
    const { pinhole, source, hostname } = this.context.data
    const label = this.form.getRawValue().label

    this.tasks.run(async () => {
      if (pinhole) {
        const { gua, externalPort } = pinhole
        await this.api.updatePinholeLabel({ gua, externalPort, label })
      } else {
        await this.api.updateForwardLabel({ source, label, hostname })
      }
      this.context.$implicit.complete()
    })
  }
}

export const PUBLISHED_PORTS_EDIT_LABEL = new PolymorpheusComponent(
  PublishedPortsEditLabel,
)
