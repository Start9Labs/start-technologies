import { DatePipe } from '@angular/common'
import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute, Router, RouterLink } from '@angular/router'
import {
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
import { TuiButton, TuiIcon, TuiLoader } from '@taiga-ui/core'
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
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { OSService } from 'src/app/services/os.service'
import { BackupTargetEntry } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { formatDuration } from 'src/app/utils/format-duration'
import { BACKUP } from './backup.component'
import { BackupService } from './backup.service'
import { BackupRun, EMPTY_HISTORY } from './backup.types'
import { CONVERSIONS } from './conversion'
import { LEGACY_BACKUP } from './legacy.component'
import { BACKUP_PICKER, Picked, PickerMode } from './picker'
import { BackupProgressComponent } from './progress.component'
import { RECOVER } from './recover.component'

@Component({
  template: `
    <ng-container *title>
      <div>
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
      </div>
    </ng-container>

    @if (backingUp()) {
      <section backupProgress></section>
    }

    <section class="g-card">
      <header>
        {{ 'Automatic backups' | i18n }}
        <a tuiButton size="s" routerLink="automatic">
          {{ 'Manage schedules' | i18n }}
        </a>
      </header>
    </section>

    <section class="g-card">
      <header>
        {{ 'Backups' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/backups.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>
        <button
          tuiButton
          size="xs"
          iconStart="@tui.copy-plus"
          class="end"
          [disabled]="backingUp()"
          (click)="create()"
        >
          {{ 'Create' | i18n }}
        </button>
        <button
          tuiButton
          size="xs"
          appearance="primary-grayscale"
          iconStart="@tui.archive-restore"
          (click)="restore()"
        >
          {{ 'Restore' | i18n }}
        </button>
      </header>
      <table [appTable]="['Started', 'Duration', 'Target', 'Result', null]">
        @for (run of history()?.runs ?? []; track run.id) {
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
            <td colspan="5">
              @if (history()) {
                {{ 'No backups yet' | i18n }}
              } @else {
                <tui-loader size="s" />
              }
            </td>
          </tr>
        }
      </table>
      <tui-table-pagination
        [total]="history()?.total ?? 0"
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
    }

    td tui-icon {
      font-size: 1rem;
      vertical-align: text-bottom;
    }

    tui-table-pagination {
      margin-block-start: 0.5rem;
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
        grid-row: 1 / span 4;
        align-self: center;
      }
    }
  `,
  imports: [
    DatePipe,
    RouterLink,
    TuiTablePagination,
    DocsLinkDirective,
    TuiButton,
    TuiIcon,
    TuiLoader,
    TuiMapperPipe,
    TableComponent,
    TitleDirective,
    BackupProgressComponent,
    i18nPipe,
  ],
})
export default class Backups {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly errors = inject(ErrorService)
  private readonly dialogs = inject(DialogService)
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
        this.api.rpc.backup.history
          .list({ offset: index * size, limit: size })
          .catch(error => {
            this.errors.handleError(error as HttpError)
            return EMPTY_HISTORY
          }),
      ),
    ),
  )

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
    this.service.recheckStale()
  }

  protected paginate({ page, size }: TuiTablePaginationEvent) {
    this.router.navigate([], {
      queryParams: { page: page || null, size: size === 10 ? null : size },
      queryParamsHandling: 'merge',
    })
  }

  protected report({ report, completedAt }: BackupRun) {
    this.dialogs
      .openComponent(REPORT, {
        data: { content: report, createdAt: completedAt },
      })
      .subscribe()
  }

  protected create() {
    this.pick('create').subscribe(({ targetId }) => {
      const target = this.service.targets()[targetId]
      if (target) this.backup(targetId, target)
    })
  }

  protected restore() {
    this.pick('restore').subscribe(({ targetId, serverId }) =>
      this.decrypt(targetId, serverId),
    )
  }

  private pick(mode: PickerMode) {
    return this.dialogs.openComponent<Picked>(BACKUP_PICKER, {
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
          : { size: 'm', data: { id } },
      )
      .subscribe()
  }

  private decrypt(targetId: string, serverId: string) {
    this.dialogs
      .openPrompt<string>({
        label: 'Password required',
        data: {
          message:
            'Enter the master password that was used to encrypt this backup. Next, you will choose which services and shared volumes to restore. Nothing on this server changes until then.',
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
