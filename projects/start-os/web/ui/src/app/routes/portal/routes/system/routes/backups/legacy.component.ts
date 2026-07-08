import { Component } from '@angular/core'
import { ConvertBytesPipe, i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiDialogContext,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

export interface LegacyBackupData {
  available: number | null
}

@Component({
  template: `
    <div tuiNotification appearance="warning" class="header">
      <span tuiTitle>
        <strong>{{ 'New Backup Format' | i18n }}</strong>
        <span tuiSubtitle>
          {{
            'A major performance improvement to StartOS backups has changed the backup format. The existing backup on this drive — the old "StartOSBackups" folder — is now obsolete and can no longer be updated. Your new backup will be created in a separate "StartOSBackupsV2" folder.'
              | i18n
          }}
        </span>
      </span>
    </div>

    <div tuiNotification appearance="positive">
      <span tuiTitle>
        <strong>{{ 'Back up everything you need' | i18n }}</strong>
        <span tuiSubtitle>
          {{
            'Because the old backup will no longer be updated, be sure to select ANY and ALL services you want backed up. Once this new backup completes successfully, DELETE the old "StartOSBackups" folder from the drive to free up space — do NOT delete "StartOSBackupsV2".'
              | i18n
          }}
        </span>
      </span>
    </div>

    <div tuiNotification appearance="warning">
      <span tuiTitle>
        <strong>{{ 'Check your available space' | i18n }}</strong>
        <span tuiSubtitle>
          {{
            'Make sure the services you select will fit within the free space remaining on this target.'
              | i18n
          }}
        </span>
        @if (data.available !== null) {
          <span tuiSubtitle>
            <strong>{{ 'Free space:' | i18n }}</strong>
            {{ data.available | convertBytes }}
          </span>
        }
      </span>
    </div>

    <footer class="g-buttons">
      <button tuiButton appearance="flat" (click)="context.completeWith(false)">
        {{ 'Cancel' | i18n }}
      </button>
      <button tuiButton (click)="context.completeWith(true)">
        {{ 'Continue' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }

    .header strong {
      font-size: 1.1rem;
    }

    footer {
      margin-top: 0.5rem;
    }
  `,
  imports: [TuiButton, TuiNotification, TuiTitle, ConvertBytesPipe, i18nPipe],
})
export class LegacyBackupModal {
  readonly context =
    injectContext<TuiDialogContext<boolean, LegacyBackupData>>()
  readonly data = this.context.data
}

export const LEGACY_BACKUP = new PolymorpheusComponent(LegacyBackupModal)
