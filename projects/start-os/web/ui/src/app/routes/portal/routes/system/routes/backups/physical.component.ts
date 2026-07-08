import { Component, inject, output } from '@angular/core'
import { ActivatedRoute } from '@angular/router'
import { ConvertBytesPipe, DialogService, i18nPipe } from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'
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
          <td>{{ driveName(target.entry) }}</td>
          <td>{{ formatCapacity(target.entry.capacity) }}</td>
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
        <tr>
          <td colspan="6">
            <app-placeholder icon="@tui.save-off">
              {{ 'No drives detected' | i18n }}
              <button
                tuiButton
                iconStart="@tui.refresh-cw"
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

    td.actions {
      text-align: right;
      white-space: nowrap;
    }

    :host-context(tui-root._mobile) {
      tr {
        grid-template-columns: min-content 1fr 4rem;
        white-space: nowrap;
      }

      td {
        grid-column: span 2;

        &:first-child {
          font-size: 0;
          width: auto;
          grid-area: 1 / 2;
          place-content: center;
          margin: 0 0.5rem;
        }

        &:last-child {
          grid-area: 1 / 3 / 4 / 3;
          align-self: center;
          justify-self: end;
        }
      }

      .name {
        color: var(--tui-text-primary);
        font: var(--tui-typography-body-m);
        font-weight: bold;
        grid-column: 1;
        max-width: 12rem;
      }
    }
  `,
  host: { class: 'g-card' },
  imports: [
    TuiButton,
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

  readonly service = inject(BackupService)
  readonly physicalFolders = output<MappedBackupTarget<DiskBackupTarget>>()

  driveName(entry: DiskBackupTarget): string {
    return (
      [entry.vendor, entry.model].filter(Boolean).join(' ') ||
      this.i18n.transform('Unknown Drive')
    )
  }

  formatCapacity(bytes: number): string {
    const gb = bytes / 1e9
    if (gb >= 1000) {
      return `${(gb / 1000).toFixed(1)} TB`
    }
    return `${gb.toFixed(0)} GB`
  }

  select(target: MappedBackupTarget<DiskBackupTarget>) {
    if (this.type === 'restore' && !target.hasAnyBackup) {
      this.dialog
        .openAlert('Drive partition does not contain a valid backup')
        .subscribe()
    } else {
      this.physicalFolders.emit(target)
    }
  }
}
