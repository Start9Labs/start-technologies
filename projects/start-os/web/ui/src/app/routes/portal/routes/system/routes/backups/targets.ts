import { DatePipe, KeyValuePipe } from '@angular/common'
import { Component, inject } from '@angular/core'
import { RouterLink } from '@angular/router'
import {
  ConvertBytesPipe,
  DialogService,
  DocsLinkDirective,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { TUI_BREAKPOINT, TuiButton, TuiHint, TuiIcon } from '@taiga-ui/core'
import { TuiTooltip } from '@taiga-ui/kit'
import { filter, firstValueFrom } from 'rxjs'

import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { AddSourceService } from 'src/app/routes/portal/routes/system/routes/volumes/add'
import { DELETE_LOCAL } from 'src/app/routes/portal/routes/system/routes/volumes/delete-local'
import { SourcePickerData } from 'src/app/routes/portal/routes/system/routes/volumes/source-picker'
import {
  VOLUME_STATUSES,
  VOLUME_TYPES,
  volumeName,
  volumePath,
} from 'src/app/routes/portal/routes/system/routes/volumes/types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { BackupTargetEntry } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { BackupService } from './backup.service'
import { CONVERSIONS, ConversionLabel } from './conversion'
import { BackupLegacyWarningComponent } from './legacy-warning.component'
import { BackupTargetManage } from './manage'

const TARGET_SOURCES: SourcePickerData = {
  intro:
    'A backup target is where StartOS stores encrypted backups of this server, its services and its shared volumes.',
  docs: '/start-os/backups.html#backup-targets',
  sources: [
    {
      kind: 'local',
      icon: '@tui.folder',
      title: 'Local folder',
      description: 'A new folder on this server’s data drive.',
      points: [
        'Easiest to set up',
        'Lost if this server’s drive fails',
        'Deleted when you remove the target',
      ],
    },
    {
      kind: 'disk',
      icon: '@tui.hard-drive',
      title: 'External drive',
      description: 'A drive connected to this server.',
      points: [
        'Survives a failure of this server',
        'Kept when you remove the target',
      ],
    },
    {
      kind: 'cifs',
      icon: '@tui.network',
      title: 'CIFS share',
      description: 'A shared folder on a NAS or another computer.',
      points: [
        'Keeps backups on another device',
        'Kept when you remove the target',
      ],
    },
  ],
}

@Component({
  template: `
    <ng-container *title>
      <div>
        <a tuiIconButton routerLink=".." iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'Backup Targets' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/backups.html"
          fragment="#backup-targets"
          appearance="icon"
          iconStart="@tui.book-open-text"
        ></a>
      </div>
    </ng-container>

    <section class="g-card">
      <header>
        {{ 'Backup Targets' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/backups.html"
          fragment="#backup-targets"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
        <button
          tuiButton
          size="xs"
          iconStart="@tui.plus"
          class="end"
          (click)="add()"
        >
          {{ 'Add' | i18n }}
        </button>
      </header>
      <table
        [appTable]="[
          'Name',
          'Type',
          'Path',
          'Status',
          'Last backup',
          'Free',
          null,
        ]"
      >
        @for (entry of service.targets() | keyvalue: null; track entry.key) {
          @let id = entry.key;
          @let target = entry.value;
          @let conversion =
            target.startOs
              | backupConversionLabel
                : service.conversions()[id]
                : service.serverId();
          <tr>
            <td>{{ name(target) }}</td>
            <td>
              <tui-icon
                [icon]="types[target.source.kind].icon"
                [tuiHint]="types[target.source.kind].label | i18n"
                [tuiHintShowDelay]="128"
              />
              <span class="mobile-path">{{ path(target.source) }}</span>
            </td>
            <td class="path">{{ path(target.source) }}</td>
            <td
              [class.g-positive]="target.availability === 'available'"
              [class.g-negative]="target.availability === 'unavailable'"
            >
              {{ statuses[target.availability] | i18n }}
              @if (target.error; as error) {
                <tui-icon [tuiTooltip]="error" />
              }
            </td>
            <td [attr.data-label]="'Last backup' | i18n">
              @if (target.startOs[service.serverId()]; as backup) {
                {{ backup.timestamp | date: 'medium' }}
              } @else {
                &mdash;
              }
            </td>
            <td [attr.data-label]="'Free' | i18n">
              @if (target.available !== null) {
                {{ target.available | convertBytes }}
              } @else {
                &mdash;
              }
            </td>
            <td>
              @if (conversion) {
                @if (breakpoint() === 'mobile') {
                  <button
                    tuiIconButton
                    size="s"
                    appearance="warning"
                    iconStart="@tui.shield-alert"
                    (click)="convert(id)"
                  >
                    {{ conversion | i18n }}
                  </button>
                } @else {
                  <button
                    tuiButton
                    size="s"
                    appearance="warning"
                    iconStart="@tui.shield-alert"
                    (click)="convert(id)"
                  >
                    {{ conversion | i18n }}
                  </button>
                }
              } @else if (target.legacyBackup) {
                <backup-legacy-warning [id]="id" />
              }
              <button
                tuiIconButton
                size="s"
                appearance="flat-grayscale"
                iconStart="@tui.settings"
                (click)="manage(id)"
              >
                {{ 'Manage' | i18n }}
              </button>
            </td>
          </tr>
        } @empty {
          <tr>
            <td colspan="7">{{ 'No backup targets' | i18n }}</td>
          </tr>
        }
      </table>
    </section>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }

    .end {
      margin-inline-start: auto;
    }

    td:last-child:not([colspan]) {
      text-align: end;
      white-space: nowrap;

      > button:not(:last-child),
      > backup-legacy-warning {
        margin-inline-end: 0.5rem;
      }
    }

    td tui-icon {
      font-size: 1rem;
      vertical-align: text-bottom;
    }

    .mobile-path {
      display: none;
      margin-inline-start: 0.375rem;
    }

    :host-context(tui-root._mobile) {
      .g-card > header > [docsLink] {
        display: none;
      }

      tr {
        grid-template-columns: 1fr auto;
        row-gap: 0.25rem;
      }

      td {
        grid-column: 1;
      }

      td[data-label]::before {
        content: attr(data-label) ': ';
      }

      td:last-child:not([colspan]) {
        grid-column: 2;
        grid-row: 1 / span 5;
        align-self: center;
      }

      td.path {
        display: none;
      }

      .mobile-path {
        display: inline;
      }
    }
  `,
  imports: [
    DatePipe,
    RouterLink,
    KeyValuePipe,
    ConvertBytesPipe,
    DocsLinkDirective,
    TuiButton,
    TuiHint,
    TuiIcon,
    TuiTooltip,
    TableComponent,
    TitleDirective,
    BackupLegacyWarningComponent,
    ConversionLabel,
    i18nPipe,
  ],
})
export default class BackupTargets {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(DialogService)
  private readonly formDialog = inject(FormDialogService)
  private readonly addSource = inject(AddSourceService)

  protected readonly breakpoint = inject(TUI_BREAKPOINT)

  protected readonly service = inject(BackupService)
  protected readonly types = VOLUME_TYPES
  protected readonly statuses = VOLUME_STATUSES
  protected readonly name = volumeName
  protected readonly path = volumePath

  constructor() {
    this.service.recheckStale()
  }

  protected add() {
    this.addSource.open('Add backup target', TARGET_SOURCES, params =>
      this.api.rpc.backup.target.create(params),
    )
  }

  protected manage(id: string) {
    this.formDialog
      .open<unknown, boolean>(BackupTargetManage, {
        label: 'Backup Target',
        data: { id },
      })
      .pipe(filter(Boolean))
      .subscribe(() => {
        const target = this.service.targets()[id]
        if (target) this.remove(id, target)
      })
  }

  protected convert(targetId: string) {
    this.dialogs
      .openComponent(CONVERSIONS, { size: 's', data: { targetId } })
      .subscribe()
  }

  private async remove(id: string, target: BackupTargetEntry) {
    const local = target.source.kind === 'local'
    const confirmed = await firstValueFrom(
      local
        ? this.dialogs.openComponent<boolean>(DELETE_LOCAL, {
            label: 'Delete permanently',
            size: 's',
            data: {
              name: target.name,
              warning:
                'Deleting this target permanently erases its folder on this server and every backup in it.',
            },
          })
        : this.dialogs.openConfirm({
            label: 'Remove',
            data: {
              content:
                'Remove this backup target? Backups on the external drive or CIFS share will not be deleted.',
              yes: 'Remove',
              no: 'Cancel',
            },
          }),
      { defaultValue: false },
    )
    if (!confirmed) return
    await this.tasks.run(
      () => this.api.rpc.backup.target.remove({ id }),
      local ? 'Deleting' : 'Removing',
    )
  }
}
