import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiCell,
  TuiCheckbox,
  TuiDialogContext,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

export interface PreserveOverwriteData {
  isExt4: boolean
  // Why the data cannot be preserved, or null when it can
  blockedReason: i18nKey | null
}

@Component({
  imports: [
    FormsModule,
    TuiButton,
    TuiCell,
    TuiCheckbox,
    TuiHeader,
    TuiNotification,
    TuiTitle,
    i18nPipe,
  ],
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2 [id]="context.id">{{ 'StartOS Data Detected' | i18n }}</h2>
        <p>{{ 'This drive contains existing StartOS data.' | i18n }}</p>
      </hgroup>
    </header>
    @if (context.data.blockedReason; as reason) {
      <div tuiNotification appearance="negative" size="m">
        <span tuiTitle>
          <strong>{{ 'Cannot Preserve Data' | i18n }}</strong>
          <span tuiSubtitle>{{ reason | i18n }}</span>
        </span>
      </div>
    }
    <ul>
      <li>
        <strong class="g-positive">{{ 'Preserve' | i18n }}</strong>
        {{ 'to keep your data.' | i18n }}
      </li>
      <li>
        <strong class="g-negative">{{ 'Overwrite' | i18n }}</strong>
        {{ 'to discard' | i18n }}
      </li>
    </ul>
    @if (context.data.isExt4) {
      <p tuiNotification appearance="warning" size="m">
        {{
          'This drive uses ext4 and will be automatically converted to btrfs. A backup is strongly recommended before proceeding.'
            | i18n
        }}
      </p>
      <label tuiCell>
        <input tuiCheckbox type="checkbox" [(ngModel)]="backupAck" />
        <span tuiTitle>{{ 'I have a backup of my data' | i18n }}</span>
      </label>
    }
    <footer>
      <button
        tuiButton
        appearance="flat-destructive"
        (click)="context.completeWith(false)"
      >
        {{ 'Overwrite' | i18n }}
      </button>
      <button
        tuiButton
        appearance=""
        [style.background]="'var(--tui-status-positive)'"
        [disabled]="
          !!context.data.blockedReason || (context.data.isExt4 && !backupAck)
        "
        (click)="context.completeWith(true)"
      >
        {{ 'Preserve' | i18n }}
      </button>
    </footer>
  `,
})
export class PreserveOverwriteDialog {
  protected readonly context =
    injectContext<TuiDialogContext<boolean, PreserveOverwriteData>>()
  protected backupAck = false
}

export const PRESERVE_OVERWRITE = new PolymorpheusComponent(
  PreserveOverwriteDialog,
)
