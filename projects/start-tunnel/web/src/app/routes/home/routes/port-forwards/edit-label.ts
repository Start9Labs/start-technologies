import { Component, inject } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { ErrorService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext, TuiError, TuiInput } from '@taiga-ui/core'
import { TuiNotificationMiddleService } from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
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
        <label tuiLabel>Label</label>
        <input tuiInput formControlName="label" />
      </tui-textfield>
      <tui-error formControlName="label" />
      <footer>
        <button tuiButton [disabled]="form.invalid" (click)="onSave()">
          Save
        </button>
      </footer>
    </form>
  `,
  imports: [ReactiveFormsModule, TuiButton, TuiError, TuiInput, TuiForm],
})
export class PortForwardsEditLabel {
  private readonly api = inject(ApiService)
  private readonly loading = inject(TuiNotificationMiddleService)
  private readonly errorService = inject(ErrorService)

  protected readonly context =
    injectContext<TuiDialogContext<void, EditLabelData>>()

  protected readonly form = inject(NonNullableFormBuilder).group({
    label: [this.context.data.label, Validators.required],
  })

  protected async onSave() {
    const loader = this.loading.open('').subscribe()

    try {
      const { pinhole, source, hostname } = this.context.data
      const label = this.form.getRawValue().label
      if (pinhole) {
        await this.api.updatePinholeLabel({
          gua: pinhole.gua,
          externalPort: pinhole.externalPort,
          label,
        })
      } else {
        await this.api.updateForwardLabel({ source, label, hostname })
      }
    } catch (e: any) {
      console.error(e)
      this.errorService.handleError(e)
    } finally {
      loader.unsubscribe()
      this.context.$implicit.complete()
    }
  }
}

export const PORT_FORWARDS_EDIT_LABEL = new PolymorpheusComponent(
  PortForwardsEditLabel,
)
