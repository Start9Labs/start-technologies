import { Component, inject, input, viewChild } from '@angular/core'
import { ActivatedRoute, Router, RouterLink } from '@angular/router'
import {
  DialogService,
  getErrorMessage,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { TuiButton, TuiCell, TuiTitle } from '@taiga-ui/core'
import { filter, switchMap, take } from 'rxjs'

import { BackupHistory } from '../../../backups/history'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { TitleDirective } from 'src/app/services/title.service'
import { BackupService } from './backup.service'
import { CHECKPOINT_RECOVER } from './checkpoint-recover'
import { BACKUP_PICKER, Picked } from './picker'
import { ScheduledBackups } from './scheduled'

@Component({
  template: `
    <ng-container *title>{{ 'Automatic backups' | i18n }}</ng-container>
    <section class="g-card">
      <header>
        <a
          tuiIconButton
          size="s"
          routerLink="/system/backups"
          iconStart="@tui.arrow-left"
        >
          {{ 'Back' | i18n }}
        </a>
        {{ 'Automatic backups' | i18n }}
      </header>
      <a tuiCell routerLink="/system/backup-targets">
        <span tuiTitle>{{ 'Backup Targets' | i18n }}</span>
      </a>
      <button tuiCell type="button" (click)="restore()">
        <span tuiTitle>{{ 'Restore from a backup' | i18n }}</span>
      </button>
      <p>
        {{
          'Automatic checkpoints include service-owned data. Back up shared volumes separately from Backups.'
            | i18n
        }}
      </p>
      <section
        scheduledBackups
        mode="manage"
        [createRequest]="
          createSchedule() === true || createSchedule() === 'true'
        "
        [reviewPackageId]="addService() || ''"
        (createRequestHandled)="handledCreateRequest()"
      ></section>
    </section>
    <section class="g-card">
      <header>{{ 'Backup history' | i18n }}</header>
      <backup-history />
    </section>
  `,
  host: { class: 'g-wrap-content' },
  imports: [
    RouterLink,
    TuiButton,
    TuiCell,
    TuiTitle,
    TitleDirective,
    i18nPipe,
    ScheduledBackups,
    BackupHistory,
  ],
})
export default class AutomaticBackups {
  private readonly router = inject(Router)
  private readonly route = inject(ActivatedRoute)
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly i18n = inject(i18nPipe)
  private readonly service = inject(BackupService)
  private readonly schedule = viewChild(ScheduledBackups)

  public readonly addService = input('')
  public readonly createSchedule = input<string | boolean>(false)

  canDeactivate(): Promise<boolean> | boolean {
    return this.schedule()?.confirmDiscardChanges() ?? true
  }

  protected handledCreateRequest() {
    this.router.navigate([], {
      relativeTo: this.route,
      queryParams: { createSchedule: null },
      queryParamsHandling: 'merge',
    })
  }

  protected restore() {
    this.dialogs
      .openComponent<Picked>(BACKUP_PICKER, { size: 'm', data: 'checkpoint' })
      .subscribe(({ targetId, serverId }) => this.decrypt(targetId, serverId))
  }

  private decrypt(targetId: string, selectedServer: string) {
    const serverId =
      this.service.targets()[targetId]?.startOs[selectedServer]?.serverId ||
      selectedServer
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
            const params = { targetId, serverId, password }
            const [manual, automatic] = await Promise.allSettled([
              this.api.rpc.backup.target.info(params),
              this.api.rpc.backup.checkpoint.discover(params),
            ])
            if (manual.status === 'rejected' && automatic.status === 'rejected')
              throw manual.reason
            const backupInfo =
              manual.status === 'fulfilled'
                ? manual.value
                : {
                    version: '',
                    timestamp: null,
                    packageBackups: {},
                    sharedVolumeBackups: {},
                  }
            const scheduledHistories =
              automatic.status === 'fulfilled' ? automatic.value : []
            if (
              !Object.keys(backupInfo.packageBackups).length &&
              !scheduledHistories.length
            )
              throw new Error(
                this.i18n.transform('No restorable checkpoints were found'),
              )
            const sources = Object.values(
              this.service.targets()[targetId]?.startOs || {},
            )
            this.dialogs
              .openComponent(CHECKPOINT_RECOVER, {
                label: 'Select services',
                data: {
                  targetId,
                  serverId,
                  password,
                  backupInfo,
                  scheduledHistories,
                  unavailableSources: [
                    manual.status === 'rejected' &&
                    sources.some(source => !source.scheduled)
                      ? `${this.i18n.transform('Manual backups could not be opened')}: ${getErrorMessage(manual.reason)}`
                      : '',
                    automatic.status === 'rejected' &&
                    sources.some(source => source.scheduled)
                      ? `${this.i18n.transform('Automatic backups could not be opened')}: ${getErrorMessage(automatic.reason)}`
                      : '',
                  ].filter(Boolean),
                },
              })
              .subscribe()
          }, 'Decrypting drive'),
        ),
        filter(Boolean),
        take(1),
      )
      .subscribe()
  }
}
