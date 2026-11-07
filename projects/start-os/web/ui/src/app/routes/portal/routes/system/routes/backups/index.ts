import { DatePipe, KeyValuePipe } from '@angular/common'
import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute, Router, RouterLink } from '@angular/router'
import {
  ConvertBytesPipe,
  DialogService,
  DocsLinkDirective,
  ErrorService,
  HttpError,
  i18nKey,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import {
  TuiTablePagination,
  TuiTablePaginationEvent,
} from '@taiga-ui/addon-table'
import { TuiMapperPipe } from '@taiga-ui/cdk'
import { TuiButton, TuiCell, TuiIcon } from '@taiga-ui/core'
import { TuiTooltip } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import {
  combineLatest,
  filter,
  firstValueFrom,
  map,
  switchMap,
  take,
} from 'rxjs'

import { REPORT } from 'src/app/components/backup-report.component'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { AddSourceService } from 'src/app/routes/portal/routes/system/routes/volumes/add'
import { SourcePickerData } from 'src/app/routes/portal/routes/system/routes/volumes/source-picker'
import {
  isStale,
  VOLUME_STATUSES,
  VOLUME_TYPES,
  volumeLocation,
} from 'src/app/routes/portal/routes/system/routes/volumes/types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { OSService } from 'src/app/services/os.service'
import {
  BackupTargetEntry,
  DataModel,
} from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { formatDuration } from 'src/app/utils/format-duration'
import { BACKUP } from './backup.component'
import { BackupService } from './backup.service'
import { BackupRun, EMPTY_HISTORY, historyRpc, targetRpc } from './backup.types'
import { CONVERSIONS, ConversionLabel } from './conversion'
import { BackupLegacyWarningComponent } from './legacy-warning.component'
import { LEGACY_BACKUP } from './legacy.component'
import { BackupTargetManage } from './manage'
import { BACKUP_PICKER, Picked, PickerMode } from './picker'
import { BackupProgressComponent } from './progress.component'
import { RECOVER } from './recover.component'
import { DELETE_LOCAL } from 'src/app/routes/portal/routes/system/routes/volumes/delete-local'

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
      <a tuiIconButton routerLink=".." iconStart="@tui.arrow-left">
        {{ 'Back' | i18n }}
      </a>
      {{ 'Backups' | i18n }}
      <a
        tuiIconButton
        size="xs"
        docsLink
        path="/start-os/backups.html"
        appearance="icon"
        iconStart="@tui.book-open-text"
      ></a>
    </ng-container>

    @if (backingUp()) {
      <section backupProgress></section>
    }

    <section class="g-card">
      <header>
        {{ 'Targets' | i18n }}
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
        [appTable]="['Name', 'Location', 'Status', 'Last backup', 'Free', null]"
      >
        @for (entry of service.targets() | keyvalue: null; track entry.key) {
          @let id = entry.key;
          @let target = entry.value;
          <tr>
            <td>{{ target.name }}</td>
            <td>
              <div tuiCell="s">
                <tui-icon [icon]="types[target.source.kind].icon" />
                {{ location(target.source) || ('This server' | i18n) }}
              </div>
            </td>
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
              @if (
                target.startOs
                  | backupConversionLabel
                    : service.conversions()[id]
                    : service.serverId();
                as label
              ) {
                <button
                  tuiButton
                  size="s"
                  appearance="warning"
                  iconStart="@tui.shield-alert"
                  (click)="convert(id)"
                >
                  {{ label | i18n }}
                </button>
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
            <td colspan="6">{{ 'No backup targets' | i18n }}</td>
          </tr>
        }
      </table>
    </section>

    <section class="g-card">
      <header>
        {{ 'History' | i18n }}
        <button
          tuiButton
          size="xs"
          iconStart="@tui.copy-plus"
          class="end"
          [disabled]="backingUp()"
          (click)="create()"
        >
          {{ 'Create backup' | i18n }}
        </button>
        <button
          tuiButton
          size="xs"
          appearance="secondary-grayscale"
          iconStart="@tui.archive-restore"
          (click)="restore()"
        >
          {{ 'Restore' | i18n }}
        </button>
      </header>
      <table [appTable]="['Started', 'Duration', 'Target', 'Result', null]">
        @for (run of history().runs; track run.id) {
          @let result = run | tuiMapper: toResult;
          <tr>
            <td>{{ run.startedAt | date: 'medium' }}</td>
            <td [attr.data-label]="'Duration' | i18n">
              {{ run | tuiMapper: toDuration }}
            </td>
            <td [attr.data-label]="'Target' | i18n">{{ run.targetName }}</td>
            <td [class]="result.class">
              <tui-icon [icon]="result.icon" />
              {{ result.label | i18n }}
            </td>
            <td>
              <button
                tuiIconButton
                size="s"
                appearance="flat-grayscale"
                iconStart="@tui.file-text"
                (click)="report(run)"
              >
                {{ 'Report' | i18n }}
              </button>
            </td>
          </tr>
        } @empty {
          <tr>
            <td colspan="5">{{ 'No backups yet' | i18n }}</td>
          </tr>
        }
      </table>
      <tui-table-pagination
        [total]="history().total"
        [page]="page().index"
        [size]="page().size"
        (paginationChange)="paginate($event)"
      />
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

    header > button + button {
      margin-inline-start: 0.5rem;
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

    [tuiCell] {
      margin: 0;
      padding: 0;
      min-block-size: 0;
      gap: 0.75rem;

      tui-icon {
        font-size: 1.25rem;
      }
    }

    tui-table-pagination {
      margin-block-start: 0.5rem;
    }

    :host-context(tui-root._mobile) {
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
    }
  `,
  imports: [
    DatePipe,
    RouterLink,
    TuiTablePagination,
    KeyValuePipe,
    ConvertBytesPipe,
    DocsLinkDirective,
    TuiButton,
    TuiCell,
    TuiIcon,
    TuiMapperPipe,
    TuiTooltip,
    TableComponent,
    TitleDirective,
    BackupLegacyWarningComponent,
    ConversionLabel,
    BackupProgressComponent,
    i18nPipe,
  ],
})
export default class Backups {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly errors = inject(ErrorService)
  private readonly dialogs = inject(DialogService)
  private readonly formDialog = inject(FormDialogService)
  private readonly addSource = inject(AddSourceService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly router = inject(Router)

  protected readonly service = inject(BackupService)
  private readonly os = inject(OSService)
  protected readonly backingUp = toSignal(this.os.backingUp$, {
    initialValue: false,
  })
  private readonly page$ = inject(ActivatedRoute).queryParamMap.pipe(
    map(params => ({
      index: Number(params.get('page') ?? 0),
      size: Number(params.get('size') ?? 10),
    })),
  )
  protected readonly page = toSignal(this.page$, {
    initialValue: { index: 0, size: 10 },
  })
  protected readonly history = toSignal(
    combineLatest([
      this.page$,
      this.os.backingUp$.pipe(filter(backingUp => !backingUp)),
    ]).pipe(
      switchMap(([{ index, size }]) =>
        historyRpc(this.api)
          .list({ offset: index * size, limit: size })
          .catch(error => {
            this.errors.handleError(error as HttpError)
            return EMPTY_HISTORY
          }),
      ),
    ),
    { initialValue: EMPTY_HISTORY },
  )
  protected readonly types = VOLUME_TYPES
  protected readonly statuses = VOLUME_STATUSES
  protected readonly location = volumeLocation

  protected readonly toDuration = ({ startedAt, completedAt }: BackupRun) =>
    formatDuration(Date.parse(completedAt) - Date.parse(startedAt))

  protected readonly toResult = ({
    report,
  }: BackupRun): { label: i18nKey; icon: string; class: string } => {
    if (report.server.error)
      return { label: 'Failed', icon: '@tui.circle-x', class: 'g-negative' }
    return [
      ...Object.values(report.packages),
      ...Object.values(report.sharedVolumes),
    ].some(item => item.error)
      ? {
          label: 'Completed with errors',
          icon: '@tui.circle-alert',
          class: 'g-warning',
        }
      : { label: 'Succeeded', icon: '@tui.circle-check', class: 'g-positive' }
  }

  constructor() {
    this.recheckStale()
  }

  private async recheckStale() {
    const targets = await firstValueFrom(this.patch.watch$('backupTargets'))
    await Promise.all(
      Object.entries(targets)
        .filter(([, { checkedAt }]) => isStale(checkedAt))
        .map(([id]) =>
          targetRpc(this.api)
            .inspect({ id })
            .catch(error => this.errors.handleError(error as HttpError)),
        ),
    )
  }

  protected add() {
    this.addSource.open('Add backup target', TARGET_SOURCES, params =>
      targetRpc(this.api).create(params),
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

  protected paginate({ page, size }: TuiTablePaginationEvent) {
    this.router.navigate([], {
      queryParams: { page: page || null, size: size === 10 ? null : size },
      queryParamsHandling: 'merge',
    })
  }

  protected convert(targetId: string) {
    this.dialogs
      .openComponent(CONVERSIONS, { size: 's', data: { targetId } })
      .subscribe()
  }

  protected report({ report, completedAt }: BackupRun) {
    this.dialogs
      .openComponent(REPORT, {
        label: 'Backup Report',
        data: { content: report, createdAt: completedAt },
        size: 'l',
      })
      .subscribe()
  }

  protected create() {
    this.pick('create', 'Create backup').subscribe(({ targetId }) => {
      const target = this.service.targets()[targetId]
      if (target) this.backup(targetId, target)
    })
  }

  protected restore() {
    this.pick('restore', 'Restore').subscribe(({ targetId, serverId }) =>
      this.decrypt(targetId, serverId),
    )
  }

  private pick(mode: PickerMode, label: i18nKey) {
    return this.dialogs.openComponent<Picked>(BACKUP_PICKER, {
      label,
      size: 'm',
      data: mode,
    })
  }

  private async backup(id: string, target: BackupTargetEntry) {
    if (!(await this.confirmLegacy(target))) return
    const readiness = await this.service.prepareBackup(id, target.startOs)
    if (readiness === 'cancelled') return
    this.dialogs
      .openComponent(
        readiness === 'conversion' ? CONVERSIONS : BACKUP,
        readiness === 'conversion'
          ? { size: 's', data: { targetId: id } }
          : { label: 'Select backup items', size: 'm', data: { id } },
      )
      .subscribe()
  }

  private decrypt(targetId: string, serverId: string) {
    this.dialogs
      .openPrompt<string>({
        label: 'Password required',
        data: {
          message:
            'Enter the master password that was used to encrypt this backup.',
          label: 'Master Password',
          placeholder: 'Enter master password',
          useMask: true,
        },
      })
      .pipe(
        filter(Boolean),
        switchMap(password =>
          this.tasks.run(async () => {
            const backupInfo = await this.api.rpc.backup.target.info({
              targetId,
              serverId,
              password,
            })
            this.dialogs
              .openComponent(RECOVER, {
                label: 'Select backup items',
                data: { targetId, serverId, backupInfo, password },
              })
              .subscribe()
          }, 'Decrypting drive'),
        ),
        filter(Boolean), // a password the server rejects leaves the prompt open to retry
        take(1),
      )
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
      () => targetRpc(this.api).remove({ id }),
      local ? 'Deleting' : 'Removing',
    )
  }

  private confirmLegacy(target: BackupTargetEntry): Promise<boolean> {
    if (!target.legacyBackup) return Promise.resolve(true)
    return firstValueFrom(
      this.dialogs.openComponent<boolean>(LEGACY_BACKUP, {
        label: 'Important!',
        size: 'm',
        data: { available: target.available },
      }),
      { defaultValue: false },
    )
  }
}
