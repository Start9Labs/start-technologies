import { CommonModule } from '@angular/common'
import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { Router } from '@angular/router'
import { i18nKey, i18nPipe, TaskService } from '@start9labs/shared'
import { Version } from '@start9labs/start-core'
import { TuiMapperPipe } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiCheckbox,
  TuiDialogContext,
  TuiGroup,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiBlock } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { map, take } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { ConfigService } from 'src/app/services/config.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { RecoverData, RecoverOption } from './backup.types'

@Component({
  template: `
    @if (packageData(); as options) {
      <div tuiGroup orientation="vertical" [collapsed]="true">
        @for (option of options; track $index) {
          <label tuiBlock>
            <span tuiTitle>
              <strong>{{ option.title }}</strong>
              <span tuiSubtitle>
                {{ 'Version' | i18n }} {{ option.version }}
              </span>
              <span tuiSubtitle>
                {{ 'Backup made' | i18n }}:
                {{ option.timestamp | date: 'medium' }}
              </span>
              @if (option | tuiMapper: toMessage; as message) {
                <span [style.color]="message.color">
                  {{ message.text | i18n }}
                </span>
              }
            </span>
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
          {{ 'Restore selected' | i18n }}
        </button>
      </footer>
    }
  `,
  styles: `
    [tuiGroup] {
      width: 100%;
      margin: 1.5rem 0 0;
    }
  `,
  imports: [
    CommonModule,
    FormsModule,
    TuiButton,
    TuiGroup,
    TuiMapperPipe,
    TuiCheckbox,
    TuiBlock,
    TuiTitle,
    i18nPipe,
  ],
})
export class BackupsRecoverComponent {
  private readonly config = inject(ConfigService)
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly router = inject(Router)
  private readonly context =
    injectContext<TuiDialogContext<void, RecoverData>>()

  readonly packageData = toSignal(
    inject<PatchDB<DataModel>>(PatchDB)
      .watch$('packageData')
      .pipe(
        take(1),
        map(packageData => {
          const backups = this.context.data.backupInfo.packageBackups

          return Object.keys(backups)
            .map(id => ({
              ...backups[id]!,
              id,
              installed: !!packageData[id],
              checked: false,
              newerOs:
                Version.parse(backups[id]?.osVersion || '').compare(
                  Version.parse(this.config.version),
                ) === 'greater',
            }))
            .sort((a, b) =>
              b.title.toLowerCase() > a.title.toLowerCase() ? -1 : 1,
            )
        }),
      ),
  )

  readonly toMessage = ({
    newerOs,
    installed,
    title,
  }: RecoverOption): { text: i18nKey; color: string } => {
    if (newerOs) {
      return {
        text: 'Unavailable. Backup was made on a newer version of StartOS.',
        color: 'var(--tui-status-negative)',
      }
    }

    if (installed) {
      return {
        text: 'Unavailable. Service is already installed.',
        color: 'var(--tui-status-warning)',
      }
    }

    return {
      text: 'Ready to restore',
      color: 'var(--tui-status-positive)',
    }
  }

  isDisabled(options: RecoverOption[]): boolean {
    return options.every(o => !o.checked)
  }

  async restore(options: RecoverOption[]): Promise<void> {
    const ids = options.filter(({ checked }) => !!checked).map(({ id }) => id)
    const { targetId, serverId, password } = this.context.data
    const params = { ids, targetId, password, serverId }

    this.tasks.run(async () => {
      await this.api.rpc.package.backup.restore(params)
      this.context.$implicit.complete()
      this.router.navigate(['services'])
    }, 'Initializing')
  }
}

export const RECOVER = new PolymorpheusComponent(BackupsRecoverComponent)
