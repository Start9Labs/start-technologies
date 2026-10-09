import { AsyncPipe } from '@angular/common'
import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import {
  TuiButton,
  TuiCheckbox,
  TuiGroup,
  TuiLoader,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiBlock } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { filter, map, switchMap, take } from 'rxjs'
import { PkgIconPipe } from 'src/app/routes/portal/pipes/pkg-icon'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { BackupContext } from './backup.types'

interface Package {
  id: string
  title: string
  disabled: boolean
  checked: boolean
}

@Component({
  template: `
    <div tuiGroup orientation="vertical" [collapsed]="true">
      @if (pkgs(); as pkgs) {
        @for (pkg of pkgs; track $index) {
          <label tuiBlock="m">
            <img alt="" [src]="pkg.id | pkgIcon | async" />
            <span tuiTitle>{{ pkg.title }}</span>
            <input
              type="checkbox"
              tuiCheckbox
              [disabled]="pkg.disabled"
              [(ngModel)]="pkg.checked"
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
      <button tuiButton [disabled]="!hasSelection" (click)="done()">
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

    img {
      width: 2.5rem;
      border-radius: 100%;
    }
  `,
  imports: [
    FormsModule,
    TuiButton,
    TuiGroup,
    TuiLoader,
    TuiBlock,
    TuiCheckbox,
    TuiTitle,
    i18nPipe,
    AsyncPipe,
    PkgIconPipe,
  ],
})
export class BackupsBackupComponent {
  private readonly dialog = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  readonly context = injectContext<BackupContext>()

  hasSelection = false
  readonly pkgs = toSignal<readonly Package[] | null>(
    this.patch.watch$('packageData').pipe(
      take(1),
      map(pkgs =>
        Object.values(pkgs)
          .map(pkg => {
            const { id, title } = getManifest(pkg)
            return {
              id,
              title,
              disabled: pkg.stateInfo.state !== 'installed',
              checked: false,
            }
          })
          .sort((a, b) =>
            b.title.toLowerCase() > a.title.toLowerCase() ? -1 : 1,
          ),
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
    this.hasSelection = !!this.pkgs()?.some(p => p.checked)
  }

  toggleSelectAll() {
    this.pkgs()?.forEach(p => (p.checked = !this.hasSelection && !p.disabled))
    this.hasSelection = !this.hasSelection
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
        this.pkgs()
          ?.filter(p => p.checked)
          .map(p => p.id) || [],
      oldPassword,
      password,
    }

    return this.tasks.run(async () => {
      try {
        await this.api.createBackup(params)
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
