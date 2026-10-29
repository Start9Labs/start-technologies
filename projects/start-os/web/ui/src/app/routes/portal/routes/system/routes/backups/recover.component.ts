import { CommonModule } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
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
    @if (options(); as options) {
      <div tuiGroup orientation="vertical" [collapsed]="true">
        @for (option of options; track option.kind + ':' + option.id) {
          <label tuiBlock>
            <span tuiTitle>
              <strong>{{ option.title }}</strong>
              @if (option.kind === 'package') {
                <span tuiSubtitle>
                  {{ 'Version' | i18n }} {{ option.version }}
                </span>
              } @else {
                <span tuiSubtitle>{{ 'Volumes' | i18n }}</span>
                <span tuiSubtitle>
                  {{
                    'Restore creates a new local volume and leaves existing data untouched.'
                      | i18n
                  }}
                </span>
              }
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
              [disabled]="
                option.kind === 'package' &&
                (option.installed || option.newerOs)
              "
              [ngModel]="selected().has(option)"
              (ngModelChange)="toggle(option, $event)"
            />
          </label>
        }
      </div>

      <footer class="g-buttons">
        <button tuiButton [disabled]="!selected().size" (click)="restore()">
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

  protected readonly selected = signal<ReadonlySet<RecoverOption>>(new Set())
  protected readonly options = toSignal(
    inject<PatchDB<DataModel>>(PatchDB)
      .watch$('packageData')
      .pipe(
        take(1),
        map(packageData => {
          const backups = this.context.data.backupInfo.packageBackups

          return [
            ...Object.entries(backups).map<RecoverOption>(([id, info]) => ({
              ...info,
              kind: 'package',
              id,
              installed: !!packageData[id],
              newerOs:
                Version.parse(info.osVersion).compare(
                  Version.parse(this.config.version),
                ) === 'greater',
            })),
            ...Object.entries(
              this.context.data.backupInfo.sharedVolumeBackups ?? {},
            ).map<RecoverOption>(([id, info]) => ({
              kind: 'volume',
              id,
              title: info.name,
              timestamp: info.timestamp,
            })),
          ].sort((a, b) => a.title.localeCompare(b.title))
        }),
      ),
  )

  readonly toMessage = (
    option: RecoverOption,
  ): { text: i18nKey; color: string } => {
    if (option.kind === 'package' && option.newerOs) {
      return {
        text: 'Unavailable. Backup was made on a newer version of StartOS.',
        color: 'var(--tui-status-negative)',
      }
    }

    if (option.kind === 'package' && option.installed) {
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

  protected toggle(option: RecoverOption, checked: boolean) {
    this.selected.update(current => {
      const selected = new Set(current)
      if (checked) selected.add(option)
      else selected.delete(option)
      return selected
    })
  }

  private deselect(options: readonly RecoverOption[]) {
    const completed = new Set(options)
    this.selected.update(
      current => new Set([...current].filter(option => !completed.has(option))),
    )
  }

  protected async restore(): Promise<void> {
    const options = [...this.selected()]
    const packages = options.filter(option => option.kind === 'package')
    const volumes = options.filter(option => option.kind === 'volume')
    const { targetId, serverId, password } = this.context.data

    await this.tasks.run(async () => {
      if (packages.length) {
        await this.api.rpc.package.backup.restore({
          targetId,
          serverId,
          password,
          ids: packages.map(option => option.id),
        })
        this.deselect(packages)
      }
      if (volumes.length) {
        const result = await this.api.rpc.backup.volume.restore(
          {
            targetId,
            serverId,
            password,
            ids: volumes.map(option => option.id),
          },
          { timeout: 0, headers: { 'ngsw-bypass': 'true' } },
        )
        this.deselect(volumes.filter(option => result.restored[option.id]))
        if (Object.keys(result.errors).length) {
          throw volumes
            .filter(option => result.errors[option.id])
            .map(option => `${option.title}: ${result.errors[option.id]}`)
            .join('<br />')
        }
      }
      this.context.$implicit.complete()
      await this.router.navigate(
        packages.length ? ['/services'] : ['/system/volumes'],
      )
    }, 'Initializing')
  }
}

export const RECOVER = new PolymorpheusComponent(BackupsRecoverComponent)
