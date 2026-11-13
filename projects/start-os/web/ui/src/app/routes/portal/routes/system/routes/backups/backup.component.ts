import { Component, computed, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiCheckbox,
  TuiCell,
  TuiIcon,
  TuiLoader,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiAvatar } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { filter, map, switchMap, take } from 'rxjs'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import {
  VOLUME_TYPES,
  volumeName,
  volumePath,
} from 'src/app/routes/portal/routes/system/routes/volumes/types'
import { BackupContext } from './backup.types'
import { BACKUP_LIST_STYLES } from './list.styles'

type BackupItem = {
  id: string
  title: string
  disabled: boolean
  checked: boolean
} & ({ kind: 'package' } | { kind: 'volume'; source: T.SharedVolumeSource })

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">{{ 'Select backup items' | i18n }}</h2>
    </header>
    @if (items(); as items) {
      @if (volumes().length) {
        <div class="group">
          <h3 id="backup-volumes">{{ 'Shared Volumes' | i18n }}</h3>
          <input
            type="checkbox"
            tuiCheckbox
            aria-labelledby="backup-volumes"
            [disabled]="!selectable(volumes())"
            [ngModel]="groupState(volumes())"
            (ngModelChange)="toggleGroup(volumes(), $event)"
          />
        </div>
        @for (item of volumes(); track item.id) {
          <label tuiCell>
            <span
              tuiAvatar="@tui.folder-symlink"
              size="s"
              aria-hidden="true"
            ></span>
            <span tuiTitle>
              <b>{{ item.title }}</b>
              @if (item.kind === 'volume') {
                <span tuiSubtitle>
                  <tui-icon [icon]="types[item.source.kind].icon" />
                  {{ path(item.source) }}
                </span>
              }
            </span>
            @if (item.disabled) {
              <span class="unavailable">{{ 'Unavailable' | i18n }}</span>
            }
            <input
              type="checkbox"
              tuiCheckbox
              [disabled]="item.disabled"
              [(ngModel)]="item.checked"
              (ngModelChange)="handleChange()"
            />
          </label>
        }
      }
      @if (packages().length) {
        <div class="group">
          <h3 id="backup-services">{{ 'Services' | i18n }}</h3>
          <input
            type="checkbox"
            tuiCheckbox
            aria-labelledby="backup-services"
            [disabled]="!selectable(packages())"
            [ngModel]="groupState(packages())"
            (ngModelChange)="toggleGroup(packages(), $event)"
          />
        </div>
        @for (item of packages(); track item.id) {
          <label tuiCell>
            <span tuiAvatar size="s" aria-hidden="true">
              <img [pkgIcon]="item.id" />
            </span>
            <span tuiTitle>
              <b>{{ item.title }}</b>
            </span>
            <input
              type="checkbox"
              tuiCheckbox
              [disabled]="item.disabled"
              [(ngModel)]="item.checked"
              (ngModelChange)="handleChange()"
            />
          </label>
        }
      }
      @if (!items.length) {
        <p>{{ 'No services installed' | i18n }}</p>
      }
    } @else {
      <tui-loader />
    }
    <footer class="g-buttons">
      <button tuiButton [disabled]="!hasSelection()" (click)="done()">
        {{ 'Done' | i18n }}
      </button>
    </footer>
  `,
  styles: [
    BACKUP_LIST_STYLES,
    `
      .unavailable {
        font: var(--tui-typography-body-s);
        color: var(--tui-text-secondary);
      }

      p {
        margin: 0;
        color: var(--tui-text-secondary);
      }
    `,
  ],
  imports: [
    FormsModule,
    TuiButton,
    TuiCell,
    TuiAvatar,
    TuiLoader,
    TuiHeader,
    TuiCheckbox,
    TuiIcon,
    TuiTitle,
    i18nPipe,
    PkgIconDirective,
  ],
})
export class BackupsBackupComponent {
  private readonly dialog = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  readonly context = injectContext<BackupContext>()
  protected readonly types = VOLUME_TYPES
  protected readonly path = volumePath

  protected readonly hasSelection = signal(false)
  protected readonly items = toSignal(
    this.patch.watch$().pipe(
      take(1),
      map(({ packageData, sharedVolumes }) =>
        [
          ...Object.values(packageData).map<BackupItem>(pkg => {
            const { id, title } = getManifest(pkg)
            return {
              kind: 'package',
              id,
              title,
              disabled: pkg.stateInfo.state !== 'installed',
              checked: false,
            }
          }),
          ...Object.entries(sharedVolumes).map<BackupItem>(([id, volume]) => ({
            kind: 'volume',
            id,
            title: volumeName(volume),
            source: volume.source,
            disabled: volume.availability === 'unavailable',
            checked: false,
          })),
        ].sort((a, b) => a.title.localeCompare(b.title)),
      ),
    ),
    { initialValue: null },
  )
  protected readonly volumes = computed(
    () => this.items()?.filter(item => item.kind === 'volume') ?? [],
  )
  protected readonly packages = computed(
    () => this.items()?.filter(item => item.kind === 'package') ?? [],
  )

  done() {
    this.dialog
      .openPrompt<string>({
        label: 'Master password needed',
        data: {
          message: 'Enter your master password to encrypt this backup.',
          label: 'Master Password',
          placeholder: 'Enter master password',
          useMask: true,
          buttonText: 'Create Backup',
        },
      })
      .pipe(
        filter(Boolean),
        switchMap(password => this.createBackup(password)),
        filter(Boolean), // a password the server rejects leaves the prompt open to retry
        take(1),
      )
      .subscribe()
  }

  handleChange() {
    this.hasSelection.set(!!this.items()?.some(item => item.checked))
  }

  selectable(items: BackupItem[]) {
    return items.some(item => !item.disabled)
  }

  groupState(items: BackupItem[]) {
    const enabled = items.filter(item => !item.disabled)
    const checked = enabled.filter(item => item.checked).length
    return !checked ? false : checked === enabled.length ? true : null
  }

  toggleGroup(items: BackupItem[], checked: boolean) {
    items.forEach(item => (item.checked = checked && !item.disabled))
    this.handleChange()
  }

  private oldPassword(password: string) {
    this.dialog
      .openPrompt<string>({
        label: 'Original password needed',
        data: {
          message:
            'This backup was created with a different password. Enter the original password that was used to encrypt this backup.',
          label: 'Original Password',
          placeholder: 'Enter original password',
          useMask: true,
          buttonText: 'Create Backup',
        },
      })
      .pipe(
        filter(Boolean),
        switchMap(oldPassword => this.createBackup(password, oldPassword)),
        filter(Boolean),
        take(1),
      )
      .subscribe()
  }

  private createBackup(password: string, oldPassword: string | null = null) {
    const params = {
      targetId: this.context.data.id,
      packageIds:
        this.items()
          ?.filter(item => item.kind === 'package' && item.checked)
          .map(item => item.id) || [],
      sharedVolumeIds:
        this.items()
          ?.filter(item => item.kind === 'volume' && item.checked)
          .map(item => item.id) || [],
      oldPassword,
      password,
    }

    return this.tasks.run(async () => {
      try {
        await this.api.rpc.backup.create(params)
      } catch (e: any) {
        if (oldPassword || e.code !== BACKUP_PASSWORD_MISMATCH) throw e

        return this.oldPassword(password)
      }

      this.context.$implicit.complete()
    }, 'Beginning backup')
  }
}

export const BACKUP = new PolymorpheusComponent(BackupsBackupComponent)

// start-core ErrorKind::BackupPasswordMismatch
const BACKUP_PASSWORD_MISMATCH = 81
