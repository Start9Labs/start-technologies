import { Component, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiDialogContext,
  TuiInput,
  TuiNotification,
} from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

interface DeleteLocalData {
  name: string
  warning: i18nKey
}

@Component({
  template: `
    <div tuiNotification appearance="negative" icon="@tui.triangle-alert">
      {{ context.data.warning | i18n }}
      <strong>{{ 'This cannot be undone.' | i18n }}</strong>
    </div>
    <p>
      {{ 'To confirm, type the name' | i18n }}
      <strong>{{ context.data.name }}</strong>
    </p>
    <tui-textfield>
      <label tuiLabel>{{ 'Name' | i18n }}</label>
      <input
        tuiInput
        autocomplete="off"
        [ngModel]="typed()"
        (ngModelChange)="typed.set($event)"
      />
    </tui-textfield>
    <footer>
      <button
        tuiButton
        type="button"
        appearance="flat-grayscale"
        (click)="context.completeWith(false)"
      >
        {{ 'Cancel' | i18n }}
      </button>
      <button
        tuiButton
        type="button"
        appearance="primary-destructive"
        iconStart="@tui.trash-2"
        [disabled]="typed().trim() !== context.data.name"
        (click)="context.completeWith(true)"
      >
        {{ 'Delete permanently' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }

    p {
      margin: 0;
    }

    footer {
      display: flex;
      justify-content: flex-end;
      gap: 1rem;
    }
  `,
  imports: [FormsModule, TuiButton, TuiInput, TuiNotification, i18nPipe],
})
class DeleteLocal {
  protected readonly context =
    injectContext<TuiDialogContext<boolean, DeleteLocalData>>()
  protected readonly typed = signal('')
}

export const DELETE_LOCAL = new PolymorpheusComponent(DeleteLocal)
