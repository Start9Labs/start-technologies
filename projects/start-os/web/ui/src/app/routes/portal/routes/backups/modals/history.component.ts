import { CommonModule } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { ErrorService, TaskService } from '@start9labs/shared'
import { TUI_FALSE_HANDLER, TUI_TRUE_HANDLER } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiCheckbox,
  TuiDialogService,
  TuiIcon,
  TuiLink,
} from '@taiga-ui/core'
import { TuiSkeleton } from '@taiga-ui/kit'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { from } from 'rxjs'
import { REPORT } from 'src/app/components/backup-report.component'
import { BackupRun } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DurationPipe } from '../pipes/duration.pipe'
import { GetBackupIconPipe } from '../pipes/get-backup-icon.pipe'
import { HasErrorPipe } from '../pipes/has-error.pipe'

@Component({
  template: `
    <h3 class="g-title">
      Past Events
      @if (selectedCount) {
        <button
          tuiButton
          class="delete"
          appearance="primary-destructive"
          (click)="delete()"
        >
          Delete Selected
        </button>
      }
    </h3>
    <table class="g-table">
      <thead>
        <tr>
          <th class="g-table-select">
            <input
              type="checkbox"
              size="s"
              tuiCheckbox
              [disabled]="!selected.length"
              [ngModel]="all"
              (ngModelChange)="toggle()"
            />
          </th>
          @if (selectedCount) {
            <th [attr.colspan]="5">
              <span class="g-table-group">
                <button
                  tuiButton
                  size="xs"
                  appearance="flat-destructive"
                  iconStart="@tui.trash"
                  (click)="delete()"
                >
                  Delete
                </button>
                <span class="count">{{ selectedCount }} selected</span>
              </span>
            </th>
          } @else {
            <th>Started At</th>
            <th>Duration</th>
            <th>Job</th>
            <th>Result</th>
            <th>Target</th>
          }
        </tr>
      </thead>
      <tbody>
        @for (run of runs(); track $index) {
          <tr>
            <td class="checkbox">
              <input
                type="checkbox"
                tuiCheckbox
                size="s"
                [(ngModel)]="selected[$index]"
              />
            </td>
            <td class="date">{{ run.startedAt | date: 'medium' }}</td>
            <td class="duration">
              {{ run.startedAt | duration: run.completedAt }} minutes
            </td>
            <td class="title">{{ run.job.name || 'No job' }}</td>
            <td class="result">
              @if (run.report | hasError) {
                <tui-icon icon="@tui.x" class="g-negative" />
              } @else {
                <tui-icon icon="@tui.check" class="g-positive" />
              }
              <button tuiLink (click)="showReport(run)">Report</button>
            </td>
            <td [style.grid-column]="'span 2'">
              @if (targets()?.saved?.[run.job.targetId]; as target) {
                <tui-icon [icon]="target.type | getBackupIcon" />
                {{ target.name }}
              }
            </td>
          </tr>
        } @empty {
          @if (runs()) {
            <tr>
              <td colspan="6">No backups have been run yet.</td>
            </tr>
          } @else {
            @for (row of ['', '']; track $index) {
              <tr>
                <td colspan="6"><div [tuiSkeleton]="true">Loading</div></td>
              </tr>
            }
          }
        }
      </tbody>
    </table>
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    tui-icon {
      font-size: 1rem;
      vertical-align: sub;
      margin-inline-end: 0.25rem;
    }

    button {
      position: relative;
    }

    [tuiCheckbox] {
      display: block;
    }

    :host-context(tui-root._mobile) {
      tr {
        grid-template-columns: 1fr 7rem;
      }

      td:only-child {
        grid-column: span 2;
      }

      .checkbox {
        @include taiga.fullsize();

        [tuiCheckbox] {
          @include taiga.fullsize();
          opacity: 0;
        }
      }

      .title {
        font-weight: bold;
        text-transform: uppercase;
      }

      .date,
      .duration {
        order: 1;
        color: var(--tui-text-secondary);
      }

      .duration,
      .result {
        text-align: right;
      }
    }

    :host-context(tui-root:not(._mobile)) .delete {
      display: none;
    }
  `,
  imports: [
    CommonModule,
    FormsModule,
    TuiButton,
    TuiIcon,
    TuiLink,
    DurationPipe,
    HasErrorPipe,
    GetBackupIconPipe,
    TuiCheckbox,
    TuiSkeleton,
  ],
})
export class BackupsHistoryModal {
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(TuiDialogService)
  private readonly errorService = inject(ErrorService)
  private readonly tasks = inject(TaskService)

  readonly targets = toSignal(from(this.api.rpc.backup.target.list({})))
  readonly runs = signal<BackupRun[] | null>(null)
  selected: boolean[] = []

  get all(): boolean | null {
    if (this.selected.length === 0) return false

    const response = this.selected[0]

    for (let i = 1; i < this.selected.length; i++) {
      if (this.selected[i] !== response) {
        return null
      }
    }

    return response
  }

  get selectedCount(): number {
    return this.selected.filter(Boolean).length
  }

  async ngOnInit() {
    try {
      this.runs.set(await this.api.getBackupRuns({}))
      this.selected = this.runs()?.map(TUI_FALSE_HANDLER) || []
    } catch (e: any) {
      this.runs.set([])
      this.errorService.handleError(e)
    }
  }

  async delete() {
    const ids = this.selected
      .filter(Boolean)
      .map((_, i) => this.runs()?.[i].id || '')

    this.tasks.run(async () => {
      await this.api.deleteBackupRuns({ ids })
      this.runs.set(this.runs()?.filter(r => !ids.includes(r.id)) || [])
      this.selected = this.runs()?.map(TUI_FALSE_HANDLER) || []
    }, 'Deleting')
  }

  showReport({ report, completedAt }: BackupRun) {
    this.dialogs
      .open(REPORT, {
        label: 'Backup Report',
        data: {
          content: report,
          createdAt: completedAt,
        },
        size: 'l',
      })
      .subscribe()
  }

  toggle() {
    if (this.all) {
      this.selected = this.selected.map(TUI_FALSE_HANDLER)
    } else {
      this.selected = this.selected.map(TUI_TRUE_HANDLER)
    }
  }
}

export const HISTORY = new PolymorpheusComponent(BackupsHistoryModal)
