import { Component, computed, inject, input, output } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute, RouterLink } from '@angular/router'
import { DialogService, DocsLinkDirective, i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiLoader, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { firstValueFrom } from 'rxjs'
import {
  CifsBackupTarget,
  DiskBackupTarget,
} from 'src/app/services/api/api.types'
import { OSService } from 'src/app/services/os.service'
import { TitleDirective } from 'src/app/services/title.service'
import { BACKUP } from './backup.component'
import { BackupService, MappedBackupTarget } from './backup.service'
import { LEGACY_BACKUP } from './legacy.component'
import { BackupLocationPicker } from '../../../backups/location-picker'
import { BackupProgressComponent } from './progress.component'
import { BACKUP_RESTORE } from './restore.component'

@Component({
  selector: 'system-backup',
  template: `
    @if (!embedded()) {
      <ng-container *title>
        <div>
          <a
            tuiIconButton
            appearance="flat-grayscale"
            iconStart="@tui.arrow-left"
            routerLink=".."
          >
            {{ 'Back' | i18n }}
          </a>
          {{
            type() === 'create'
              ? ('Create a manual backup' | i18n)
              : ('Restore from a backup' | i18n)
          }}
          <a
            tuiIconButton
            docsLink
            appearance="icon"
            iconStart="@tui.book-open-text"
            size="xs"
            [path]="
              type() === 'create'
                ? '/start-os/backup-create.html'
                : '/start-os/backup-restore.html'
            "
          ></a>
        </div>
      </ng-container>

      <header tuiHeader>
        <hgroup tuiTitle>
          <h3>
            {{
              type() === 'create'
                ? ('Create a manual backup' | i18n)
                : ('Restore from a backup' | i18n)
            }}
          </h3>
        </hgroup>
      </header>
    }

    @if (busy()) {
      @if (embedded()) {
        <div
          tuiNotification
          class="backup-busy"
          appearance="info"
          role="status"
        >
          {{ 'A backup or restore is already in progress.' | i18n }}
        </div>
      } @else {
        <section backupProgress></section>
      }
    } @else {
      @if (service.loading()) {
        <tui-loader
          textContent="Fetching backups"
          size="l"
          [style.height.rem]="20"
        />
      } @else {
        <backup-location-picker
          [mode]="type() === 'create' ? 'manual' : 'restore'"
          (selected)="onTarget($event)"
          (manage)="manageLocations.emit()"
        />
      }
    }
  `,
  styles: `
    :host {
      display: grid;
      gap: 1rem;
      inline-size: 100%;
      min-inline-size: 0;
      max-inline-size: 64rem;
      margin-inline: auto;
    }

    :host-context(tui-root._mobile) [tuiHeader] {
      display: none;
    }
  `,
  host: { class: 'g-wrap-content' },
  imports: [
    RouterLink,
    TuiButton,
    TuiLoader,
    TuiNotification,
    TuiHeader,
    TuiTitle,
    TitleDirective,
    BackupLocationPicker,
    BackupProgressComponent,
    i18nPipe,
    DocsLinkDirective,
  ],
})
export default class SystemBackupComponent {
  readonly mode = input<'create' | 'restore'>()
  readonly embedded = input(false)
  readonly operationActive = input<boolean>()
  readonly manageLocations = output<void>()
  protected readonly dialog = inject(DialogService)
  private readonly route = inject(ActivatedRoute)
  protected readonly type = computed(
    () =>
      this.mode() || (this.route.snapshot.data['type'] as 'create' | 'restore'),
  )
  protected readonly service = inject(BackupService)
  private readonly os = inject(OSService)
  private readonly progressActive = toSignal(this.os.backingUp$, {
    initialValue: false,
  })
  protected readonly busy = computed(
    () => this.operationActive() ?? this.progressActive(),
  )
  constructor() {
    this.service.getBackupTargets()
  }

  protected async onTarget(
    target: MappedBackupTarget<CifsBackupTarget | DiskBackupTarget>,
  ) {
    if (this.type() === 'create') {
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
          label: 'Unlock backup',
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
