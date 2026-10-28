import { CommonModule } from '@angular/common'
import { Component, inject } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiMapperPipe } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiCheckbox,
  TuiDialogContext,
  TuiGroup,
} from '@taiga-ui/core'
import { TuiBlock } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { take } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { ToOptionsPipe } from '../pipes/to-options.pipe'
import { RecoverData } from '../types/recover-data'
import { RecoverOption } from '../types/recover-option'

@Component({
  template: `
    @if (packageData$ | toOptions: backups | async; as options) {
      <div
        tuiGroup
        orientation="vertical"
        [collapsed]="true"
        [style.width.%]="100"
      >
        @for (option of options; track $index) {
          <label tuiBlock>
            <div [style.flex]="1" [style.margin]="'0.75rem 0'">
              <strong>{{ option.title }}</strong>
              <div>Version {{ option.version }}</div>
              <div>Backup made: {{ option.timestamp | date: 'medium' }}</div>
              @if (option | tuiMapper: toMessage; as message) {
                <div [style.color]="message.color">{{ message.text }}</div>
              }
            </div>
            <input
              type="checkbox"
              tuiCheckbox
              [disabled]="option.installed || option.newerOs"
              [(ngModel)]="option.checked"
            />
          </label>
        }
      </div>
      <footer class="g-buttons">
        <button
          tuiButton
          [disabled]="isDisabled(options)"
          (click)="restore(options)"
        >
          Restore Selected
        </button>
      </footer>
    }
  `,
  imports: [
    CommonModule,
    FormsModule,
    ToOptionsPipe,
    TuiButton,
    TuiGroup,
    TuiMapperPipe,
    TuiCheckbox,
    TuiBlock,
  ],
})
export class BackupsRecoverModal {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly context =
    injectContext<TuiDialogContext<void, RecoverData>>()

  readonly packageData$ = inject<PatchDB<DataModel>>(PatchDB)
    .watch$('packageData')
    .pipe(take(1))

  readonly toMessage = ({ newerOs, installed, title }: RecoverOption) => {
    if (newerOs) {
      return {
        text: `Unavailable. Backup was made on a newer version of StartOS.`,
        color: 'var(--tui-status-negative)',
      }
    }

    if (installed) {
      return {
        text: `Unavailable. ${title} is already installed.`,
        color: 'var(--tui-status-warning)',
      }
    }

    return {
      text: 'Ready to restore',
      color: 'var(--tui-status-positive)',
    }
  }

  get backups(): Record<string, T.PackageBackupInfo> {
    return this.context.data.backupInfo.packageBackups
  }

  isDisabled(options: RecoverOption[]): boolean {
    return options.every(o => !o.checked)
  }

  async restore(options: RecoverOption[]): Promise<void> {
    const ids = options.filter(({ checked }) => !!checked).map(({ id }) => id)
    const { targetId, serverId, password } = this.context.data

    this.tasks.run(async () => {
      await this.api.rpc.package.backup.restore({
        ids,
        targetId,
        password,
        serverId,
      })

      this.context.$implicit.complete()
    }, 'Initializing')
  }
}

export const RECOVER = new PolymorpheusComponent(BackupsRecoverModal)
