import { CommonModule } from '@angular/common'
import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { i18nKey, i18nPipe, TaskService } from '@start9labs/shared'
import { Version } from '@start9labs/start-core'
import { TuiMapperPipe } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiCheckbox,
  TuiDataList,
  TuiDialogContext,
  TuiGroup,
  TuiLabel,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiBlock, TuiChevron, TuiSelect } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { map, take } from 'rxjs'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { ConfigService } from 'src/app/services/config.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { RecoverCheckpoint, RecoverData, RecoverOption } from './backup.types'
import { SYSTEM_PACKAGE_ID } from './scheduled-utils'

@Component({
  template: `
    @if (unavailableSources.length) {
      <div tuiNotification appearance="warning">
        @for (source of unavailableSources; track source) {
          <p>{{ source }}</p>
        }
        <p>
          {{
            'Check the backup location or retry with the password used to encrypt those checkpoints.'
              | i18n
          }}
        </p>
      </div>
    }
    @if (packageData(); as options) {
      <div class="bulk-controls">
        <label class="toggle-all">
          <input
            tuiCheckbox
            type="checkbox"
            [ngModel]="allEligibleSelected(options)"
            (ngModelChange)="setAll(options, $event)"
          />
          <span tuiTitle>
            <b>{{ 'Toggle all' | i18n }}</b>
          </span>
        </label>
        <tui-textfield
          tuiChevron
          [stringify]="options | tuiMapper: stringifyBulkSelection"
          [tuiTextfieldCleaner]="false"
        >
          <label tuiLabel>{{ 'Checkpoints' | i18n }}</label>
          <input
            tuiSelect
            [disabled]="!selected(options).length"
            [attr.aria-label]="'Checkpoint for selected services' | i18n"
            [ngModel]="bulkSelection"
            (ngModelChange)="applyBulk(options, $event)"
          />
          <tui-data-list *tuiDropdown>
            <button tuiOption value="latest">
              {{ 'Latest available' | i18n }}
            </button>
            <button
              tuiOption
              value="manual"
              [disabled]="!bulkAvailable(options, 'manual')"
            >
              {{ 'Latest manual' | i18n }}
            </button>
            <button
              tuiOption
              value="automatic"
              [disabled]="!bulkAvailable(options, 'automatic')"
            >
              {{ 'Latest automatic' | i18n }}
            </button>
            @for (run of sharedRuns(options); track run.id) {
              <button
                tuiOption
                class="checkpoint-option"
                [value]="'run:' + run.id"
              >
                {{ 'Automatic' | i18n }} — {{ run.timestamp | date: 'medium' }}
              </button>
            }
          </tui-data-list>
        </tui-textfield>
      </div>
      <div tuiGroup orientation="vertical" [collapsed]="true">
        @for (option of options; track $index) {
          <label tuiBlock class="service-choice">
            <input
              type="checkbox"
              tuiCheckbox
              [disabled]="option.installed || option.newerOs"
              [(ngModel)]="option.checked"
              (ngModelChange)="selectionChanged(options)"
            />
            <span tuiTitle>
              <strong>{{ option.title }}</strong>
              @if (option | tuiMapper: toMessage; as message) {
                <span [style.color]="message.color">
                  {{ message.text | i18n }}
                </span>
              }
            </span>
            <tui-textfield
              tuiChevron
              class="checkpoint"
              [stringify]="option | tuiMapper: stringifyCheckpoint"
              [tuiTextfieldCleaner]="false"
            >
              <label tuiLabel>{{ 'Checkpoints' | i18n }}</label>
              <input
                tuiSelect
                [disabled]="option.installed || option.newerOs"
                [(ngModel)]="option.selectedKey"
              />
              <tui-data-list *tuiDropdown>
                @for (checkpoint of option.checkpoints; track checkpoint.key) {
                  <button
                    tuiOption
                    class="checkpoint-option"
                    [value]="checkpoint.key"
                  >
                    {{ checkpointLabel(checkpoint) }}
                  </button>
                }
              </tui-data-list>
            </tui-textfield>
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
    :host {
      container-type: inline-size;
    }

    .bulk-controls {
      display: flex;
      align-items: end;
      justify-content: space-between;
      gap: 1rem;
    }

    .bulk-controls label {
      display: grid;
      gap: 0.25rem;
      color: var(--tui-text-secondary);
    }

    .bulk-controls tui-textfield {
      min-inline-size: 16rem;
    }

    [tuiGroup] {
      inline-size: 100%;
      margin: 1.5rem 0 0;
    }

    .checkpoint {
      grid-column: 2;
      inline-size: 100%;
    }

    .bulk-controls .toggle-all {
      display: flex;
      align-items: flex-start;
      gap: 0.65rem;
    }

    .service-choice {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr);
      align-items: start;
      gap: 0.65rem;
    }

    .service-choice > [tuiTitle] {
      display: grid;
      gap: 0.25rem;
    }

    .checkpoint-option {
      block-size: auto;
      min-block-size: 2.75rem;
      padding-block: 0.5rem;
      white-space: normal;
    }

    .toggle-all {
      color: var(--tui-text-primary);
    }

    [tuiTitle] {
      min-inline-size: 0;
      overflow-wrap: anywhere;
    }

    @container (max-inline-size: 30rem) {
      .bulk-controls {
        align-items: stretch;
        flex-direction: column;
      }

      .bulk-controls label,
      .bulk-controls tui-textfield {
        inline-size: 100%;
        min-inline-size: 0;
      }

      .bulk-controls > .toggle-all {
        align-self: flex-start;
      }

      .checkpoint {
        grid-column: 1 / -1;
      }

      .checkpoint input {
        font-size: 0.875rem;
      }
    }
  `,
  host: { class: 'g-wrap-content' },
  imports: [
    CommonModule,
    FormsModule,
    TuiButton,
    TuiGroup,
    TuiMapperPipe,
    TuiCheckbox,
    TuiChevron,
    TuiDataList,
    TuiBlock,
    TuiLabel,
    TuiNotification,
    TuiSelect,
    TuiTitle,
    i18nPipe,
  ],
})
export class BackupsRecoverComponent {
  private readonly config = inject(ConfigService)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  private readonly tasks = inject(TaskService)
  private readonly context =
    injectContext<TuiDialogContext<void, RecoverData>>()

  protected readonly unavailableSources =
    this.context.data.unavailableSources || []

  protected readonly packageData = toSignal(
    inject<PatchDB<DataModel>>(PatchDB)
      .watch$('packageData')
      .pipe(
        take(1),
        map(packageData => {
          const backups = this.context.data.backupInfo.packageBackups
          const scheduled = this.context.data.scheduledHistories
          const ids = new Set([
            ...Object.keys(backups),
            ...scheduled.map(history => history.packageId),
          ])

          return [...ids]
            .filter(id => id !== SYSTEM_PACKAGE_ID)
            .map(id => {
              const manual = backups[id]
              const scheduledCheckpoints = scheduled
                .filter(history => history.packageId === id)
                .flatMap(history => history.snapshots)
                .map(snapshot => ({
                  key: `scheduled:${snapshot.id}`,
                  source: 'scheduled' as const,
                  version: snapshot.packageVersion,
                  timestamp: snapshot.completedAt,
                  jobName: snapshot.jobName,
                  snapshotId: snapshot.id,
                  runId: snapshot.runId,
                  archived: snapshot.archived,
                }))
              const checkpoints: RecoverCheckpoint[] = [
                ...(manual
                  ? [
                      {
                        key: 'manual',
                        source: 'manual' as const,
                        version: manual.version,
                        timestamp: manual.timestamp,
                      },
                    ]
                  : []),
                ...scheduledCheckpoints,
              ].sort((a, b) => b.timestamp.localeCompare(a.timestamp))
              const pkg = packageData[id]
              const title =
                manual?.title || (pkg && getManifest(pkg)?.title) || id
              return {
                id,
                title,
                installed: !!packageData[id],
                checked: false,
                selectedKey: checkpoints[0]?.key || '',
                checkpoints,
                newerOs:
                  !!manual &&
                  Version.parse(manual.osVersion || '').compare(
                    Version.parse(this.config.version),
                  ) === 'greater',
              }
            })
            .filter(option => option.checkpoints.length > 0)
            .sort((a, b) =>
              b.title.toLowerCase() > a.title.toLowerCase() ? -1 : 1,
            )
        }),
      ),
  )

  protected bulkSelection = 'latest'

  protected readonly stringifyBulkSelection = (options: RecoverOption[]) => {
    return (selection: string) => {
      if (selection === 'latest') {
        return this.i18n.transform('Latest available')
      }
      if (selection === 'manual') {
        return this.i18n.transform('Latest manual')
      }
      if (selection === 'automatic') {
        return this.i18n.transform('Latest automatic')
      }
      const run = this.sharedRuns(options).find(
        item => `run:${item.id}` === selection,
      )
      return run
        ? `${this.i18n.transform('Automatic')} ${this.compactDate(run.timestamp)}`
        : selection
    }
  }

  protected readonly stringifyCheckpoint = (option: RecoverOption) => {
    return (key: string) => {
      const checkpoint = option.checkpoints.find(item => item.key === key)
      if (!checkpoint) return ''
      return `${this.i18n.transform(
        checkpoint.source === 'manual' ? 'Manual' : 'Automatic',
      )} ${this.compactDate(checkpoint.timestamp)}`
    }
  }

  private compactDate(timestamp: string): string {
    return new Intl.DateTimeFormat(undefined, {
      month: 'numeric',
      day: 'numeric',
    }).format(new Date(timestamp))
  }

  protected checkpointLabel(checkpoint: RecoverCheckpoint | undefined): string {
    if (!checkpoint) return ''
    const parts = [
      this.i18n.transform(
        checkpoint.source === 'manual' ? 'Manual' : 'Automatic',
      ),
      checkpoint.jobName,
      new Date(checkpoint.timestamp).toLocaleString(),
      checkpoint.version,
      checkpoint.archived ? this.i18n.transform('Archived') : '',
    ]
    return parts.filter(Boolean).join(' — ')
  }

  protected readonly toMessage = ({
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

  protected isDisabled(options: RecoverOption[]): boolean {
    return options.every(o => !o.checked)
  }

  protected selected(options: RecoverOption[]): RecoverOption[] {
    return options.filter(option => option.checked)
  }

  protected allEligibleSelected(options: RecoverOption[]): boolean {
    const eligible = options.filter(
      option => !option.installed && !option.newerOs,
    )
    return !!eligible.length && eligible.every(option => option.checked)
  }

  protected setAll(options: RecoverOption[], checked: boolean) {
    options
      .filter(option => !option.installed && !option.newerOs)
      .forEach(option => (option.checked = checked))
    this.selectionChanged(options)
  }

  protected selectionChanged(options: RecoverOption[]) {
    const runId = this.bulkSelection.startsWith('run:')
      ? this.bulkSelection.slice(4)
      : null
    const valid =
      this.bulkSelection === 'latest' ||
      (this.bulkSelection === 'manual' &&
        this.bulkAvailable(options, 'manual')) ||
      (this.bulkSelection === 'automatic' &&
        this.bulkAvailable(options, 'automatic')) ||
      (!!runId && this.sharedRuns(options).some(run => run.id === runId))
    if (!valid) this.bulkSelection = 'latest'
    this.applyBulk(options, this.bulkSelection)
  }

  protected bulkAvailable(
    options: RecoverOption[],
    source: 'manual' | 'automatic',
  ): boolean {
    const selected = this.selected(options)
    return (
      !!selected.length &&
      selected.every(option =>
        option.checkpoints.some(checkpoint =>
          source === 'manual'
            ? checkpoint.source === 'manual'
            : checkpoint.source === 'scheduled',
        ),
      )
    )
  }

  protected sharedRuns(
    options: RecoverOption[],
  ): { id: string; timestamp: string }[] {
    const selected = this.selected(options)
    if (!selected.length) return []
    const common = selected
      .slice(1)
      .reduce(
        (ids, option) =>
          new Set(
            [...ids].filter(id =>
              option.checkpoints.some(checkpoint => checkpoint.runId === id),
            ),
          ),
        new Set(
          selected[0]!.checkpoints.flatMap(checkpoint =>
            checkpoint.runId ? [checkpoint.runId] : [],
          ),
        ),
      )
    return [...common]
      .map(id => ({
        id,
        timestamp:
          selected[0]!.checkpoints.find(checkpoint => checkpoint.runId === id)
            ?.timestamp || '',
      }))
      .sort((a, b) => b.timestamp.localeCompare(a.timestamp))
  }

  protected applyBulk(options: RecoverOption[], selection: string) {
    this.bulkSelection = selection
    const runId = selection.startsWith('run:') ? selection.slice(4) : null
    for (const option of this.selected(options)) {
      const checkpoint =
        selection === 'latest'
          ? option.checkpoints[0]
          : selection === 'manual'
            ? option.checkpoints.find(item => item.source === 'manual')
            : selection === 'automatic'
              ? option.checkpoints.find(item => item.source === 'scheduled')
              : option.checkpoints.find(item => item.runId === runId)
      if (checkpoint) option.selectedKey = checkpoint.key
    }
  }

  protected async restore(options: RecoverOption[]): Promise<void> {
    const selected = options.filter(({ checked }) => !!checked)
    const ids = selected
      .filter(option => option.selectedKey === 'manual')
      .map(({ id }) => id)
    const snapshots = Object.fromEntries(
      selected.flatMap(option => {
        const checkpoint = option.checkpoints.find(
          checkpoint => checkpoint.key === option.selectedKey,
        )
        return checkpoint?.source === 'scheduled' && checkpoint.snapshotId
          ? [[option.id, checkpoint.snapshotId]]
          : []
      }),
    )
    const { targetId, serverId, password } = this.context.data
    void this.tasks.run(async () => {
      await this.api.restoreBackupSelection({
        targetId,
        manualIds: ids,
        snapshots,
        serverId,
        password,
      })

      this.context.$implicit.complete()
    }, 'Initializing')
  }
}

export const RECOVER = new PolymorpheusComponent(BackupsRecoverComponent)
