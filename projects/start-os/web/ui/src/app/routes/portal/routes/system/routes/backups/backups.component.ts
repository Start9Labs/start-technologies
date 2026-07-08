import { AsyncPipe, DatePipe } from '@angular/common'
import { Component, inject, OnInit } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute, RouterLink } from '@angular/router'
import { DialogService, DocsLinkDirective, i18nPipe } from '@start9labs/shared'
import { TuiMapperPipe } from '@taiga-ui/cdk'
import { TuiButton, TuiLoader, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom } from 'rxjs'
import {
  CifsBackupTarget,
  DiskBackupTarget,
} from 'src/app/services/api/api.types'
import { OSService } from 'src/app/services/os.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { BACKUP } from './backup.component'
import { BackupService, MappedBackupTarget } from './backup.service'
import { LEGACY_BACKUP } from './legacy.component'
import { BackupNetworkComponent } from './network.component'
import { BackupPhysicalComponent } from './physical.component'
import { BackupProgressComponent } from './progress.component'
import { BACKUP_RESTORE } from './restore.component'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{
          type === 'create'
            ? ('Create Backup' | i18n)
            : ('Restore Backup' | i18n)
        }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          [path]="
            type === 'create'
              ? '/start-os/backup-create.html'
              : '/start-os/backup-restore.html'
          "
          appearance="icon"
          iconStart="@tui.book-open-text"
        ></a>
      </div>
    </ng-container>

    <header tuiHeader>
      <hgroup tuiTitle>
        <h3>
          {{
            type === 'create'
              ? ('Create Backup' | i18n)
              : ('Restore Backup' | i18n)
          }}
          <a
            tuiIconButton
            size="xs"
            docsLink
            [path]="
              type === 'create'
                ? '/start-os/backup-create.html'
                : '/start-os/backup-restore.html'
            "
            appearance="icon"
            iconStart="@tui.book-open-text"
          ></a>
        </h3>
      </hgroup>
    </header>

    @if (type === 'create' && !(os.backingUp$ | async) && server(); as s) {
      <div
        tuiNotification
        [appearance]="s.lastBackup | tuiMapper: toAppearance"
        icon=""
      >
        <div tuiTitle>
          {{ 'Last Backup' | i18n }}
          <div tuiSubtitle>
            {{ s.lastBackup ? (s.lastBackup | date: 'medium') : 'never' }}
          </div>
        </div>
      </div>
    }

    @if (type === 'create' && (os.backingUp$ | async)) {
      <section backupProgress></section>
    } @else {
      @if (service.loading()) {
        <tui-loader
          textContent="Fetching backups"
          size="l"
          [style.height.rem]="20"
        />
      } @else {
        <section (networkFolders)="onTarget($event)"></section>
        <section (physicalFolders)="onTarget($event)"></section>
      }
    }
  `,
  styles: `
    :host-context(tui-root._mobile) [tuiHeader] {
      display: none;
    }
  `,
  imports: [
    AsyncPipe,
    DatePipe,
    RouterLink,
    TuiButton,
    TuiLoader,
    TuiHeader,
    TuiTitle,
    TuiNotification,
    TuiMapperPipe,
    TitleDirective,
    BackupNetworkComponent,
    BackupPhysicalComponent,
    BackupProgressComponent,
    i18nPipe,
    DocsLinkDirective,
  ],
})
export default class SystemBackupComponent implements OnInit {
  readonly dialog = inject(DialogService)
  readonly type = inject(ActivatedRoute).snapshot.data['type']
  readonly service = inject(BackupService)
  readonly os = inject(OSService)
  readonly server = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('serverInfo'),
  )

  readonly toAppearance = (lastBackup: string | null) => {
    if (!lastBackup) return 'negative'

    const currentDate = new Date().valueOf()
    const backupDate = new Date(lastBackup).valueOf()
    const diff = currentDate - backupDate
    const week = 604800000

    if (diff <= week) {
      return 'positive'
    } else if (diff > week && diff <= week * 2) {
      return 'warning'
    } else {
      return 'negative'
    }
  }

  ngOnInit() {
    this.service.getBackupTargets()
  }

  async onTarget(
    target: MappedBackupTarget<CifsBackupTarget | DiskBackupTarget>,
  ) {
    if (this.type === 'create') {
      if (!(await this.confirmLegacy(target.entry))) return

      this.dialog
        .openComponent(BACKUP, {
          label: 'Select services',
          data: target,
          size: 'm',
        })
        .subscribe()
    } else {
      this.dialog
        .openComponent(BACKUP_RESTORE, {
          label: 'Select server',
          data: target,
          size: 'l',
        })
        .subscribe()
    }
  }

  private confirmLegacy(
    entry: CifsBackupTarget | DiskBackupTarget,
  ): Promise<boolean> {
    if (!entry.legacyBackup) return Promise.resolve(true)

    return firstValueFrom(
      this.dialog.openComponent<boolean>(LEGACY_BACKUP, {
        label: 'Important!',
        size: 'm',
        data: { available: entry.available },
      }),
      { defaultValue: false },
    )
  }
}
