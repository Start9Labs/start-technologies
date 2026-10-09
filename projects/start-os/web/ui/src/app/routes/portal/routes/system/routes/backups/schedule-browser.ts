import { DatePipe } from '@angular/common'
import { Component, inject, input, output } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiAppearance,
  TuiButton,
  TuiCell,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiBadge, TuiSwitch } from '@taiga-ui/kit'

import {
  backupPauseLabel,
  backupTargetName,
  BackupTargetName,
  parseBackupServiceSelection,
  SYSTEM_PACKAGE_ID,
} from './scheduled-utils'

@Component({
  selector: 'backup-schedule-browser',
  template: `
    <section class="schedule-browser">
      <div class="schedule-list">
        @for (job of jobs(); track job.id) {
          <div tuiCell class="schedule-job">
            @let selection = jobSelectionSummary(job);
            <tui-icon icon="@tui.calendar-clock" />
            <span tuiTitle>
              <b>{{ job.name }}</b>
              <span tuiSubtitle>
                {{ targetName(job.targetId) }} ·
                {{ selection.serviceCount }}
                {{ serviceCountLabel(selection.serviceCount) }}
                @if (!selection.includeFuture) {
                  · {{ 'Future services not included' | i18n }}
                }
                @if (!selection.includesSystem) {
                  · {{ 'No System data' | i18n }}
                }
                · {{ 'Next run' | i18n }}:
                {{
                  job.status.nextRunAt
                    ? (job.status.nextRunAt | date: 'medium')
                    : ('None' | i18n)
                }}
              </span>
            </span>
            @if (job.pause; as pause) {
              <span tuiBadge appearance="warning">
                {{ pauseLabel(pause) | i18n }}
              </span>
            } @else if (!job.enabled) {
              <span tuiBadge>{{ 'Paused' | i18n }}</span>
            }
            <div class="job-list-actions">
              <label class="job-switch">
                <input
                  tuiSwitch
                  type="checkbox"
                  [attr.aria-label]="job.name"
                  [ngModelOptions]="{ standalone: true }"
                  [ngModel]="job.enabled && !job.pause"
                  [disabled]="!!job.pause && job.pause.reason !== 'user'"
                  (ngModelChange)="enabledChange.emit({ job, enabled: $event })"
                />
              </label>
              <button
                tuiIconButton
                tuiDropdown
                tuiDropdownAuto
                type="button"
                size="s"
                appearance="flat-grayscale"
                iconStart="@tui.ellipsis-vertical"
              >
                {{ 'More' | i18n }}
                <tui-data-list *tuiDropdown="let close" (click)="close()">
                  <button
                    tuiOption
                    tuiAppearance="flat"
                    [disabled]="!!job.pause || !job.enabled"
                    (click)="runRequested.emit(job)"
                  >
                    {{ 'Run now' | i18n }}
                  </button>
                  <button tuiOption (click)="editRequested.emit(job)">
                    {{ 'View/Edit' | i18n }}
                  </button>
                  <button
                    tuiOption
                    tuiAppearance="flat-destructive"
                    (click)="deleteRequested.emit(job)"
                  >
                    {{ 'Delete schedule' | i18n }}
                  </button>
                </tui-data-list>
              </button>
            </div>
          </div>
        }
      </div>
    </section>

    <div class="jobs-toolbar">
      <button
        tuiButton
        type="button"
        size="s"
        appearance="primary"
        iconStart="@tui.plus"
        (click)="createRequested.emit()"
      >
        {{ 'Add schedule' | i18n }}
      </button>
    </div>
  `,
  styles: `
    :host,
    .schedule-browser,
    .schedule-list {
      display: grid;
      gap: 0.75rem;
      min-inline-size: 0;
    }

    .schedule-list > * {
      min-inline-size: 0;
      overflow-wrap: anywhere;
    }

    .schedule-job {
      display: flex;
      align-items: center;
      gap: 0.75rem;
      inline-size: 100%;
      min-inline-size: 0;
      padding: 0.75rem 0;
      border-block-end: 1px solid var(--tui-border-normal);
      text-align: start;
      box-sizing: border-box;
    }

    .schedule-job:last-child {
      border-block-end: 0;
    }

    .schedule-job [tuiTitle] {
      flex: 1;
    }

    .job-list-actions,
    .jobs-toolbar {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      gap: 0.5rem;
      min-inline-size: 0;
    }

    .job-list-actions {
      flex-wrap: wrap;
    }

    .job-switch {
      inline-size: fit-content;
    }

    @container (max-inline-size: 30rem) {
      .jobs-toolbar {
        align-items: stretch;
        flex-direction: column;
      }

      .schedule-job {
        display: grid;
        grid-template-columns: auto minmax(0, 1fr) auto;
        align-items: center;
        padding-inline: 0.75rem;
      }

      .schedule-job > tui-icon:first-child {
        grid-column: 1;
        grid-row: 1;
      }

      .schedule-job > [tuiTitle] {
        display: contents;
      }

      .schedule-job > [tuiTitle] > b {
        grid-column: 2;
        grid-row: 1;
        min-inline-size: 0;
        overflow-wrap: anywhere;
      }

      .schedule-job > [tuiTitle] > [tuiSubtitle] {
        grid-column: 1 / -1;
        grid-row: 2;
        min-inline-size: 0;
        white-space: normal;
        overflow-wrap: anywhere;
      }

      .schedule-job > [tuiBadge] {
        grid-column: 1 / -1;
        grid-row: 3;
        justify-self: start;
      }

      .job-list-actions {
        grid-column: 3;
        grid-row: 1;
        align-self: start;
        justify-self: end;
        flex-wrap: nowrap;
      }
    }

    @container (max-inline-size: 20rem) {
      .schedule-job > [tuiTitle] > b {
        grid-column: 2 / -1;
      }

      .schedule-job > [tuiBadge] {
        grid-column: 1 / 3;
      }

      .job-list-actions {
        grid-column: 3;
        grid-row: 3;
      }
    }
  `,
  imports: [
    DatePipe,
    FormsModule,
    TuiAppearance,
    TuiBadge,
    TuiButton,
    TuiCell,
    TuiDataList,
    TuiDropdown,
    TuiIcon,
    TuiSwitch,
    TuiTitle,
    i18nPipe,
  ],
})
export class BackupScheduleBrowser {
  private readonly i18n = inject(i18nPipe)

  readonly jobs = input.required<readonly T.BackupJob[]>()
  readonly packageIds = input.required<readonly string[]>()
  readonly targets = input.required<readonly BackupTargetName[]>()
  readonly enabledChange = output<{ job: T.BackupJob; enabled: boolean }>()
  readonly runRequested = output<T.BackupJob>()
  readonly editRequested = output<T.BackupJob>()
  readonly deleteRequested = output<T.BackupJob>()
  readonly createRequested = output<void>()

  protected readonly pauseLabel = backupPauseLabel
  protected readonly targetName = (id: string) =>
    backupTargetName(this.targets(), id)

  protected jobSelectionSummary(job: T.BackupJob): {
    serviceCount: number
    includeFuture: boolean
    includesSystem: boolean
  } {
    const selection = parseBackupServiceSelection(job.services, [
      ...this.packageIds(),
    ])
    const packageIds = new Set(selection.packageIds)
    const includesSystem = packageIds.delete(SYSTEM_PACKAGE_ID)
    return {
      serviceCount: packageIds.size,
      includeFuture: selection.includeFuture,
      includesSystem,
    }
  }

  protected serviceCountLabel(count: number): string {
    return this.i18n.transform(count === 1 ? 'Service' : 'Services')
  }
}
