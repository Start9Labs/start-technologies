import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import {
  TuiButton,
  TuiCheckbox,
  TuiGroup,
  TuiLoader,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiBlock } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { filter, map, switchMap, take } from 'rxjs'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { BackupContext } from './backup.types'

interface Package {
  id: string
  title: string
  icon: string
  disabled: boolean
  checked: boolean
}

@Component({
  template: `
    <div tuiNotification appearance="warning">
      {{
        'For each selected service, this replaces its previous manual checkpoint. Automatic checkpoints are not changed.'
          | i18n
      }}
    </div>
    <label class="toggle-all">
      <input
        tuiCheckbox
        type="checkbox"
        [ngModel]="allEligibleSelected()"
        (ngModelChange)="setAll($event)"
      />
      <span tuiTitle>
        <b>{{ 'Toggle all' | i18n }}</b>
      </span>
    </label>
    <div tuiGroup orientation="vertical" [collapsed]="true">
      @if (pkgs(); as pkgs) {
        @for (pkg of pkgs; track pkg.id) {
          <label tuiBlock="m">
            <input
              type="checkbox"
              tuiCheckbox
              [disabled]="pkg.disabled"
              [(ngModel)]="pkg.checked"
              (ngModelChange)="handleChange()"
            />
            <img alt="" [src]="pkg.icon" />
            <span tuiTitle>{{ pkg.title }}</span>
          </label>
        } @empty {
          {{ 'No services installed' | i18n }}
        }
      } @else {
        <tui-loader />
      }
    </div>
    <footer class="g-buttons">
      <button tuiButton [disabled]="!hasSelection" (click)="done()">
        {{ 'Done' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    [tuiGroup] {
      inline-size: 100%;
      margin: 0;
    }

    [tuiBlock] {
      align-items: center;
    }

    [tuiTitle] {
      min-inline-size: 0;
      overflow-wrap: anywhere;
    }

    img {
      inline-size: 2.5rem;
      flex-shrink: 0;
      border-radius: 100%;
    }

    .toggle-all {
      display: flex;
      align-items: center;
      gap: 0.75rem;
      padding: 1.5rem 1rem 0.75rem;
    }
  `,
  host: { class: 'g-wrap-content' },
  imports: [
    FormsModule,
    TuiButton,
    TuiGroup,
    TuiLoader,
    TuiNotification,
    TuiBlock,
    TuiCheckbox,
    TuiTitle,
    i18nPipe,
  ],
})
export class BackupsBackupComponent {
  private readonly dialog = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  protected readonly context = injectContext<BackupContext>()

  protected hasSelection = false
  protected readonly pkgs = toSignal<readonly Package[] | null>(
    this.patch.watch$('packageData').pipe(
      take(1),
      map(pkgs =>
        Object.values(pkgs)
          .map(pkg => {
            const { id, title } = getManifest(pkg)
            return {
              id,
              title,
              icon: pkg.icon,
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

  protected done() {
    this.dialog
      .openPrompt<string>({
        label: 'Master password needed',
        data: {
          message: 'Enter your master password to encrypt this backup.',
          label: 'Password',
          placeholder: 'Enter master password',
          useMask: true,
          buttonText: 'Create a manual backup',
        },
      })
      .pipe(
        filter(Boolean),
        switchMap(password => this.createBackup(password)),
        filter(Boolean),
        take(1),
      )
      .subscribe()
  }

  protected handleChange() {
    this.hasSelection = !!this.pkgs()?.some(p => p.checked)
  }

  protected allEligibleSelected(): boolean {
    const eligible = this.pkgs()?.filter(pkg => !pkg.disabled) || []
    return !!eligible.length && eligible.every(pkg => pkg.checked)
  }

  protected setAll(checked: boolean) {
    this.pkgs()?.forEach(pkg => (pkg.checked = checked && !pkg.disabled))
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
