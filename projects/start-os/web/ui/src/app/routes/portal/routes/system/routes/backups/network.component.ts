import { Component, inject, output } from '@angular/core'
import { ActivatedRoute } from '@angular/router'
import {
  ConvertBytesPipe,
  DialogService,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { ISB, T, utils } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDataList,
  TuiDialogContext,
  TuiDropdown,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { filter } from 'rxjs'

import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { CifsBackupTarget } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import {
  BackupService,
  formatCifsLocation,
  MappedBackupTarget,
} from './backup.service'
import { BackupLegacyWarningComponent } from './legacy-warning.component'
import { BackupStatusComponent } from './status.component'

const ERROR =
  'Ensure (1) target computer is connected to the same LAN as your Start9 Server, (2) target folder is being shared, and (3) hostname, path, and credentials are accurate.'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">
        {{ 'Delete network folder?' | i18n }}
      </h2>
    </header>
    <p>
      <strong>{{ context.data.name }}</strong>
    </p>
    <p>{{ 'Remove this network folder from StartOS?' | i18n }}</p>
    <p>
      {{
        'Stored backup files will remain on the network folder and can be reconnected later.'
          | i18n
      }}
    </p>
    <footer class="g-buttons">
      <button
        tuiButton
        type="button"
        appearance="flat"
        (click)="context.$implicit.complete()"
      >
        {{ 'Cancel' | i18n }}
      </button>
      <button
        tuiButton
        type="button"
        appearance="primary-destructive"
        (click)="context.completeWith(true)"
      >
        {{ 'Delete' | i18n }}
      </button>
    </footer>
  `,
  imports: [TuiButton, TuiHeader, TuiTitle, i18nPipe],
})
class NetworkDeleteDialog {
  protected readonly context =
    injectContext<TuiDialogContext<boolean, { name: string }>>()
}

const NETWORK_DELETE = new PolymorpheusComponent(NetworkDeleteDialog)

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
          <td class="name">
            <b>{{ target.entry.path.split('/').pop() }}</b>
          </td>
          <td class="hostname">{{ target.entry.hostname }}</td>
          <td class="location">{{ target.entry.path }}</td>
          <td class="free">
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
        <tr class="empty-row">
          <td class="empty-state" colspan="6">
            <app-placeholder icon="@tui.folder-x">
              <span class="empty-label">
                {{ 'No network folders' | i18n }}
              </span>
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

    td:nth-child(2) {
      inline-size: 22%;
    }

    .name,
    .hostname,
    .location,
    .free {
      justify-self: start;
      text-align: start;
    }

    .hostname,
    .location {
      overflow-wrap: anywhere;
    }

    .free {
      white-space: nowrap;
    }

    .empty-row {
      inline-size: 100%;
    }

    .empty-state {
      display: table-cell;
      block-size: 7rem;
      vertical-align: middle;
      text-align: center;
    }

    .empty-state app-placeholder {
      inline-size: 100%;
      max-inline-size: 16rem;
      margin-inline: auto;
      box-sizing: border-box;
      padding: 0;
      gap: 0.25rem;
    }

    .empty-label {
      display: block;
      inline-size: 100%;
      max-inline-size: 100%;
      min-block-size: 1.5rem;
      flex-shrink: 0;
      line-height: 1.5rem;
      overflow-wrap: anywhere;
      text-align: center;
    }

    td:last-child:not(.empty-state) {
      inline-size: 3.5rem;
      white-space: nowrap;
      text-align: end;
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
        grid-template-columns: minmax(0, 1fr) auto auto;
        inline-size: 100%;
        min-inline-size: 0;
        padding-block: 0.75rem;
        box-shadow: inset 0 -1px var(--tui-background-neutral-1);
        white-space: normal;
      }

      tr.empty-row {
        grid-template-columns: minmax(0, 1fr);
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

        &:first-child:not(:only-child) {
          inline-size: auto;
          grid-area: 4 / 1 / 5 / -1;
          justify-self: start;
          margin-block-start: 0.25rem;
        }

        &:last-child {
          grid-area: 1 / 3;
          align-self: center;
          justify-self: end;
        }
      }

      td.name {
        inline-size: auto;
        color: var(--tui-text-primary);
        font: var(--tui-typography-body-m);
        grid-area: 1 / 1;
        justify-self: stretch;
        max-inline-size: 100%;
        overflow-wrap: normal;
        text-align: start;
        word-break: normal;
      }

      td.name b {
        font-weight: bold;
      }

      td.free {
        grid-area: 1 / 2;
        align-self: center;
        justify-self: end;
      }

      td.hostname {
        grid-area: 2 / 1 / 3 / -1;
        margin-block-start: 0.25rem;
      }

      td.location {
        grid-area: 3 / 1 / 4 / -1;
      }

      .free {
        max-inline-size: 100%;
      }

      .empty-row > td.empty-state {
        display: grid;
        grid-area: 1 / 1 / auto / -1;
        place-items: center;
        justify-self: stretch;
        inline-size: auto;
        margin: 0;
        overflow: visible;
        white-space: normal;
        text-align: center;
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

  protected readonly service = inject(BackupService)
  readonly networkFolders = output<MappedBackupTarget<CifsBackupTarget>>()

  protected opens: Record<number, boolean> = {}

  protected select(target: MappedBackupTarget<CifsBackupTarget>) {
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

  protected async add() {
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

  protected async edit(target: MappedBackupTarget<CifsBackupTarget>) {
    this.formDialog.open(FormComponent, {
      label: 'Update Network Folder',
      data: {
        spec: await configBuilderToSpec(this.cifsSpec()),
        buttons: [
          {
            text: this.i18n.transform('Connect'),
            handler: async (value: T.CifsAddParams) =>
              this.connect(async () => {
                const res = await this.api.updateBackupTarget({
                  id: target.id,
                  ...value,
                })

                target.entry = Object.values(res)[0]!
                this.service.cifs.update(cifs => [...cifs])
              }),
          },
        ],
        value: { ...target.entry },
      },
    })
  }

  protected forget(
    target: MappedBackupTarget<CifsBackupTarget>,
    index: number,
  ) {
    this.dialog
      .openComponent<boolean>(NETWORK_DELETE, {
        data: { name: formatCifsLocation(target.entry) },
        size: 's',
      })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(async () => {
          await this.api.removeBackupTarget({ id: target.id })
          this.service.cifs.update(cifs => cifs.filter((_, i) => i !== index))
        }, 'Removing'),
      )
  }

  private async addTarget(v: T.CifsAddParams): Promise<boolean> {
    return this.connect(async () => {
      const [item] = Object.entries(await this.api.addBackupTarget(v))
      const [id, entry] = item || []

      if (!id || !entry) {
        throw 'Invalid response from server'
      }

      const hasAnyBackup = this.service.hasAnyBackup(entry)
      const hasCurrentBackup = this.service.hasCurrentBackup(entry)
      const added = { id, entry, hasAnyBackup, hasCurrentBackup }
      this.service.cifs.update(cifs => [added, ...cifs])
    })
  }

  private connect(task: () => Promise<void>): Promise<boolean> {
    return this.tasks.run(async () => {
      try {
        await task()
      } catch (error) {
        const technical =
          error instanceof Error
            ? error.message
            : typeof error === 'string'
              ? error
              : JSON.stringify(error) || String(error)
        const summary = this.i18n.transform(
          /permission denied|mount error\(13\)|logon failure|authentication/i.test(
            technical,
          )
            ? 'The network folder rejected the username or password. Check the credentials and sharing permissions, then try again.'
            : 'StartOS could not reach the network folder. Check the hostname, path, and network connection, then try again.',
        )
        throw new Error(
          `${summary}\n\n${this.i18n.transform('Technical details')}:\n${technical}`,
          { cause: error },
        )
      }
    }, 'Testing connectivity to shared folder')
  }

  protected cifsSpec() {
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
        patterns: [
          {
            regex: `^(${utils.regexes.hostname.contains()}|${utils.regexes.ipv6.contains()})$`,
            description: this.i18n.transform(
              'Enter a valid hostname or IP address.',
            ),
          },
        ],
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
