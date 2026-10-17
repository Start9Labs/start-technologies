import { Component, inject, output } from '@angular/core'
import { ActivatedRoute } from '@angular/router'
import { ConvertBytesPipe, DialogService, i18nPipe } from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'
import { TuiButtonLoading } from '@taiga-ui/kit'

import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { DiskBackupTarget } from 'src/app/services/api/api.types'
import { BackupService, MappedBackupTarget } from './backup.service'
import { BackupLegacyWarningComponent } from './legacy-warning.component'
import { BackupStatusComponent } from './status.component'

@Component({
  selector: '[physicalFolders]',
  template: `
    <header>
      {{ 'Physical Drives' | i18n }}
    </header>

    <table
      [appTable]="['Status', 'Logicalname', 'Name', 'Capacity', 'Free', null]"
    >
      @for (target of service.drives(); track $index) {
        <tr
          tabindex="0"
          (click)="select(target)"
          (keydown.enter)="select(target)"
        >
          <td>
            <span [backupStatus]="target.hasAnyBackup" [physical]="true"></span>
          </td>
          <td class="name">{{ target.entry.logicalname }}</td>
          <td class="location">{{ driveName(target.entry) }}</td>
          <td>{{ target.entry.capacity | convertBytes }}</td>
          <td>
            @if (target.entry.available !== null) {
              {{ target.entry.available | convertBytes }}
            } @else {
              &mdash;
            }
          </td>
          <td class="actions">
            @if (type === 'create' && target.entry.legacyBackup) {
              <backup-legacy-warning
                [id]="target.id"
                [hasCurrentBackup]="target.hasCurrentBackup"
              />
            }
          </td>
        </tr>
      } @empty {
        <tr class="empty-row">
          <td class="empty-state" colspan="6">
            <app-placeholder icon="@tui.save-off">
              {{ 'No drives detected' | i18n }}
              <button
                tuiButton
                iconStart="@tui.refresh-cw"
                [loading]="service.loading()"
                (click)="service.getBackupTargets()"
              >
                {{ 'Refresh' | i18n }}
              </button>
            </app-placeholder>
          </td>
        </tr>
      }
    </table>
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    tr {
      @include taiga.transition(background);

      @media (taiga.$tui-mouse) {
        &:not(:has(app-placeholder)):hover:not(:has(button:hover)) {
          cursor: pointer;
          background: var(--tui-background-neutral-1-hover);
        }
      }
    }

    :host {
      container-type: inline-size;
      inline-size: 100%;
      min-inline-size: 0;
    }

    table {
      inline-size: 100%;
      table-layout: fixed;
    }

    td:first-child:not(.empty-state) {
      inline-size: 15rem;
    }

    td:last-child:not(.empty-state),
    .actions {
      inline-size: 3.5rem;
      white-space: nowrap;
      text-align: end;
    }

    .name,
    .location {
      justify-self: start;
      text-align: start;
    }

    .empty-state {
      display: table-cell;
      block-size: 7rem;
      vertical-align: middle;
      text-align: center;
    }

    .empty-state app-placeholder {
      inline-size: min(100%, 16rem);
      margin-inline: auto;
      box-sizing: border-box;
    }

    @container (max-inline-size: 48rem) {
      table {
        --app-table-header-display: none;

        min-inline-size: 0;
        border: none;
        border-radius: 0;
        box-shadow: none;
        table-layout: auto;
        color: var(--tui-text-secondary);
      }

      tr {
        position: relative;
        display: grid;
        grid-template-columns: minmax(0, 1fr) minmax(7rem, 45%);
        inline-size: 100%;
        min-inline-size: 0;
        padding-block: 0.75rem;
        box-shadow: inset 0 -1px var(--tui-background-neutral-1);
        white-space: normal;
      }

      tr:last-child {
        box-shadow: none;
      }

      td {
        position: static;
        min-inline-size: 0;
        padding: 0;
        border: none;
        grid-column: span 2;
        overflow-wrap: anywhere;

        &:first-child:not(.empty-state) {
          inline-size: auto;
          grid-area: 3 / 1 / 4 / -1;
          justify-self: start;
          margin-block-start: 0.25rem;
        }

        &:nth-child(3) {
          grid-area: 2 / 1;
        }

        &:nth-child(4) {
          grid-area: 1 / 2;
        }

        &:nth-child(5) {
          grid-area: 2 / 2;
        }

        &:last-child:not(.empty-state) {
          grid-column: 1 / -1;
          inline-size: auto;
        }
      }

      .name {
        color: var(--tui-text-primary);
        font: var(--tui-typography-body-m);
        font-weight: bold;
        grid-area: 1 / 1;
        justify-self: start;
        max-inline-size: 100%;
        text-align: start;
      }

      .empty-state {
        display: grid;
        grid-column: 1 / -1;
        block-size: auto;
        min-block-size: 7rem;
        place-items: center;
        justify-self: center;
        inline-size: 100%;
        white-space: normal;
        text-align: center;
      }
    }
  `,
  host: { class: 'g-card' },
  imports: [
    TuiButton,
    TuiButtonLoading,
    PlaceholderComponent,
    BackupStatusComponent,
    BackupLegacyWarningComponent,
    TableComponent,
    ConvertBytesPipe,
    i18nPipe,
  ],
})
export class BackupPhysicalComponent {
  private readonly dialog = inject(DialogService)
  protected readonly type = inject(ActivatedRoute).snapshot.data['type']

  private readonly i18n = inject(i18nPipe)

  protected readonly service = inject(BackupService)
  readonly physicalFolders = output<MappedBackupTarget<DiskBackupTarget>>()

  protected driveName(entry: DiskBackupTarget): string {
    return (
      [entry.vendor, entry.model].filter(Boolean).join(' ') ||
      this.i18n.transform('Unknown Drive')
    )
  }

  protected select(target: MappedBackupTarget<DiskBackupTarget>) {
    if (this.type === 'restore' && !target.hasAnyBackup) {
      this.dialog
        .openAlert('Drive partition does not contain a valid backup')
        .subscribe()
    } else {
      this.physicalFolders.emit(target)
    }
  }
}
