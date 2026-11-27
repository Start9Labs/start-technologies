import { Component, inject, input } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { Router, RouterLink } from '@angular/router'
import {
  DialogService,
  DocsLinkDirective,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'
import { filter, firstValueFrom, switchMap, take } from 'rxjs'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { OSService } from 'src/app/services/os.service'
import { BackupTargetEntry } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { BackupHistory } from '../../../backups/history'
import { BACKUP } from './backup.component'
import { BackupService } from './backup.service'
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

    <section class="g-card automatic-backups">
      <header>
        {{ 'Automatic backups' | i18n }}
        <a tuiButton size="s" class="end" routerLink="automatic">
          {{ 'Manage schedules' | i18n }}
        </a>
      </header>
    </section>

    <section class="g-card">
      <header class="history-header">
        {{ 'Backup history' | i18n }}
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
      <backup-history
        [historyKind]="historyKind()"
        [historyStatus]="historyStatus()"
        [historySearch]="historySearch()"
        [historyPage]="historyPage()"
        (filtersChange)="updateHistoryFilters($event)"
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

    .automatic-backups {
      padding: 0;
    }

    .automatic-backups > header {
      position: static;
      height: auto;
      min-block-size: 3rem;
      flex-wrap: wrap;
      gap: 0.75rem 1rem;
      padding: 0.75rem 1rem;
      box-sizing: border-box;
    }

    .history-header {
      height: auto;
      min-block-size: 3rem;
      flex-wrap: wrap;
      gap: 0.75rem 1rem;
    }
  `,
  imports: [
    RouterLink,
    DocsLinkDirective,
    TuiButton,
    TitleDirective,
    BackupProgressComponent,
    BackupHistory,
    i18nPipe,
  ],
})
export default class Backups {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(DialogService)
  private readonly router = inject(Router)

  protected readonly service = inject(BackupService)
  private readonly os = inject(OSService)
  protected readonly backingUp = toSignal(this.os.backingUp$, {
    initialValue: false,
  })
  public readonly historyKind = input('')
  public readonly historyStatus = input('')
  public readonly historySearch = input('')
  public readonly historyPage = input('')

  constructor() {
    this.service.recheckStale()
  }

  protected updateHistoryFilters(
    queryParams: Record<string, string | number | null>,
  ) {
    this.router.navigate([], { queryParams, queryParamsHandling: 'merge' })
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
