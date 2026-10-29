import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiCheckbox,
  TuiGroup,
  TuiLoader,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiAvatar, TuiBlock } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { filter, map, switchMap, take } from 'rxjs'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { BackupContext } from './backup.types'

type BackupItem = {
  id: string
  title: string
  disabled: boolean
  checked: boolean
} & (
  | { kind: 'package' }
  | { kind: 'volume'; availability: T.SharedVolumeAvailability }
)

@Component({
  template: `
    <div tuiGroup orientation="vertical" [collapsed]="true">
      @if (items(); as items) {
        @for (item of items; track item.kind + ':' + item.id) {
          <label tuiBlock="m">
            <span
              size="m"
              aria-hidden="true"
              [tuiAvatar]="item.kind === 'volume' ? '@tui.folder' : ''"
            >
              @if (item.kind === 'package') {
                <img [pkgIcon]="item.id" />
              }
            </span>
            <span tuiTitle>
              <b>{{ item.title }}</b>
              @if (item.kind === 'volume') {
                <span tuiSubtitle>{{ 'Volumes' | i18n }}</span>
                @if (item.availability === 'unavailable') {
                  <span tuiSubtitle>
                    {{ 'Volume source unavailable' | i18n }}
                  </span>
                }
              }
            </span>
            <input
              type="checkbox"
              tuiCheckbox
              [disabled]="item.disabled"
              [(ngModel)]="item.checked"
              (ngModelChange)="handleChange()"
            />
          </label>
        } @empty {
          {{ 'No services installed' | i18n }}
        }
      } @else {
        <tui-loader />
      }
    </div>
    <footer class="g-buttons">
      <button tuiButton appearance="flat-grayscale" (click)="toggleSelectAll()">
        {{ 'Toggle all' | i18n }}
      </button>
      <button tuiButton [disabled]="!hasSelection()" (click)="done()">
        {{ 'Done' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    [tuiGroup] {
      width: 100%;
      margin: 1.5rem 0 0;
    }

    [tuiBlock] {
      align-items: center;
    }
  `,
  imports: [
    FormsModule,
    TuiButton,
    TuiGroup,
    TuiAvatar,
    TuiLoader,
    TuiBlock,
    TuiCheckbox,
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
            title: volume.name,
            availability: volume.availability,
            disabled: false,
            checked: false,
          })),
        ].sort((a, b) => a.title.localeCompare(b.title)),
      ),
    ),
    { initialValue: null },
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

  toggleSelectAll() {
    const checked = !this.hasSelection()
    this.items()?.forEach(item => (item.checked = checked && !item.disabled))
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
