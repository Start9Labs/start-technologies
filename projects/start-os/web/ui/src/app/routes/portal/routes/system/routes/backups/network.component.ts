import { Component, inject, output } from '@angular/core'
import { ActivatedRoute } from '@angular/router'
import {
  ConvertBytesPipe,
  DialogService,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { ISB, T } from '@start9labs/start-core'
import { TuiButton, TuiDataList, TuiDropdown, TuiIcon } from '@taiga-ui/core'
import { filter } from 'rxjs'
import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { CifsBackupTarget } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import { BackupService, MappedBackupTarget } from './backup.service'
import { BackupLegacyWarningComponent } from './legacy-warning.component'
import { BackupStatusComponent } from './status.component'

const ERROR =
  'Ensure (1) target computer is connected to the same LAN as your Start9 Server, (2) target folder is being shared, and (3) hostname, path, and credentials are accurate.'

@Component({
  selector: '[networkFolders]',
  template: `
    <header>
      {{ 'Network Folders' | i18n }}
      <button
        tuiButton
        size="xs"
        iconStart="@tui.plus"
        [style.margin-inline-start]="'auto'"
        (click)="add()"
      >
        {{ 'New' | i18n }}
      </button>
    </header>

    <table [appTable]="['Status', 'Name', 'Hostname', 'Path', 'Free', null]">
      @for (target of service.cifs(); track $index) {
        <tr
          tabindex="0"
          (click)="select(target)"
          (keydown.enter)="select(target)"
        >
          <td>
            @if (target.entry.mountable) {
              <span [backupStatus]="target.hasAnyBackup"></span>
            } @else {
              <span>
                <tui-icon
                  icon="@tui.signal-high"
                  class="g-negative"
                  [style.font-size.rem]="1"
                />
                Unable to connect
              </span>
            }
          </td>
          <td class="name">{{ target.entry.path.split('/').pop() }}</td>
          <td>{{ target.entry.hostname }}</td>
          <td>{{ target.entry.path }}</td>
          <td>
            @if (target.entry.available !== null) {
              {{ target.entry.available | convertBytes }}
            } @else {
              &mdash;
            }
          </td>
          <td>
            <div class="actions">
              @if (
                type === 'create' &&
                target.entry.mountable &&
                target.entry.legacyBackup
              ) {
                <backup-legacy-warning
                  [id]="target.id"
                  [hasCurrentBackup]="target.hasCurrentBackup"
                />
              }
              <button
                tuiIconButton
                tuiDropdown
                size="s"
                appearance="flat-grayscale"
                iconStart="@tui.ellipsis-vertical"
                [tuiDropdownOpen]="!!opens[$index]"
                (tuiDropdownOpenChange)="opens[$index] = $event"
                (click)="$event.stopPropagation()"
              >
                {{ 'More' | i18n }}
                <tui-data-list *tuiDropdown>
                  <button tuiOption (click)="edit(target)">
                    {{ 'Edit' | i18n }}
                  </button>
                  <button
                    tuiOption
                    class="g-negative"
                    (click)="forget(target, $index)"
                  >
                    {{ 'Delete' | i18n }}
                  </button>
                </tui-data-list>
              </button>
            </div>
          </td>
        </tr>
      } @empty {
        <tr>
          <td colspan="6">
            <app-placeholder icon="@tui.folder-x">
              No network folders
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

    td:first-child {
      width: 16rem;
    }

    td:last-child {
      white-space: nowrap;
      text-align: right;
    }

    .actions {
      display: inline-flex;
      align-items: center;
      gap: 0.5rem;
    }

    span {
      display: flex;
      align-items: center;
      gap: 0.25rem;
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

        &:only-child {
          grid-column: 1 / -1;
          justify-self: center;
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
    TuiDataList,
    TuiDropdown,
    TuiIcon,
    PlaceholderComponent,
    BackupStatusComponent,
    BackupLegacyWarningComponent,
    TableComponent,
    ConvertBytesPipe,
    i18nPipe,
  ],
})
export class BackupNetworkComponent {
  private readonly dialog = inject(DialogService)
  private readonly formDialog = inject(FormDialogService)
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly i18n = inject(i18nPipe)

  protected readonly type = inject(ActivatedRoute).snapshot.data['type']

  readonly service = inject(BackupService)
  readonly networkFolders = output<MappedBackupTarget<CifsBackupTarget>>()

  opens: Record<number, boolean> = {}

  select(target: MappedBackupTarget<CifsBackupTarget>) {
    if (!target.entry.mountable) {
      this.dialog.openAlert(ERROR, { label: 'Unable to connect' }).subscribe()
    } else if (this.type === 'restore' && !target.hasAnyBackup) {
      this.dialog
        .openAlert('Network Folder does not contain a valid backup')
        .subscribe()
    } else {
      this.networkFolders.emit(target)
    }
  }

  async add() {
    this.formDialog.open(FormComponent, {
      label: 'New Network Folder',
      data: {
        spec: await configBuilderToSpec(this.cifsSpec()),
        buttons: [
          {
            text: this.i18n.transform('Connect'),
            handler: (value: T.CifsAddParams) => this.addTarget(value),
          },
        ],
      },
    })
  }

  async edit(target: MappedBackupTarget<CifsBackupTarget>) {
    this.formDialog.open(FormComponent, {
      label: 'Update Network Folder',
      data: {
        spec: await configBuilderToSpec(this.cifsSpec()),
        buttons: [
          {
            text: this.i18n.transform('Connect'),
            handler: async (value: T.CifsAddParams) =>
              this.tasks.run(async () => {
                const res = await this.api.updateBackupTarget({
                  id: target.id,
                  ...value,
                })

                target.entry = Object.values(res)[0]!
                this.service.cifs.update(cifs => [...cifs])
              }, 'Testing connectivity to shared folder'),
          },
        ],
        value: { ...target.entry },
      },
    })
  }

  forget({ id }: MappedBackupTarget<CifsBackupTarget>, index: number) {
    this.dialog
      .openConfirm({ label: 'Are you sure?', size: 's' })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(async () => {
          await this.api.removeBackupTarget({ id })
          this.service.cifs.update(cifs => cifs.filter((_, i) => i !== index))
        }, 'Removing'),
      )
  }

  private async addTarget(v: T.CifsAddParams): Promise<boolean> {
    return this.tasks.run(async () => {
      const [item] = Object.entries(await this.api.addBackupTarget(v))
      const [id, entry] = item || []

      if (!id || !entry) {
        throw 'Invalid response from server'
      }

      const hasAnyBackup = this.service.hasAnyBackup(entry)
      const hasCurrentBackup = this.service.hasCurrentBackup(entry)
      const added = { id, entry, hasAnyBackup, hasCurrentBackup }
      this.service.cifs.update(cifs => [added, ...cifs])
    }, 'Testing connectivity to shared folder')
  }

  cifsSpec() {
    return ISB.InputSpec.of({
      hostname: ISB.Value.text({
        name: this.i18n.transform('Hostname')!,
        description: this.i18n.transform(
          'The hostname of your target device on the Local Area Network.',
        ),
        warning: null,
        placeholder: `e.g. 'My Computer' OR 'my-computer.local'`,
        required: true,
        default: null,
        patterns: [],
      }),
      path: ISB.Value.text({
        name: this.i18n.transform('Path')!,
        description: this.i18n.transform(
          'On Windows, this is the fully qualified path to the shared folder, (e.g. /Desktop/my-folder). On Linux and Mac, this is the literal name of the shared folder (e.g. my-shared-folder).',
        ),
        placeholder: 'e.g. my-shared-folder or /Desktop/my-folder',
        required: true,
        default: null,
      }),
      username: ISB.Value.text({
        name: this.i18n.transform('Username')!,
        description: this.i18n.transform(
          'On Linux, this is the samba username you created when sharing the folder. On Mac and Windows, this is the username of the user who is sharing the folder.',
        ),
        required: true,
        default: null,
      }),
      password: ISB.Value.text({
        name: this.i18n.transform('Password')!,
        description: this.i18n.transform(
          'On Linux, this is the samba password you created when sharing the folder. On Mac and Windows, this is the password of the user who is sharing the folder.',
        ),
        required: false,
        default: null,
        masked: true,
      }),
    })
  }
}
