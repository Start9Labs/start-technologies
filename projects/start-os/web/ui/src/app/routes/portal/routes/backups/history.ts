import { DatePipe, DecimalPipe, KeyValuePipe } from '@angular/common'
import { Component, computed, inject, signal } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { ActivatedRoute, Router } from '@angular/router'
import { convertBytes, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TUI_BREAKPOINT,
  TuiDataList,
  TuiIcon,
  TuiInput,
  TuiLabel,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import {
  TuiAccordion,
  TuiBadge,
  TuiChevron,
  TuiPagination,
  TuiSelect,
} from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { tap } from 'rxjs'

import { DataModel } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { BackupService } from '../system/routes/backups/backup.service'
import { SYSTEM_PACKAGE_ID } from '../system/routes/backups/scheduled-utils'

type HistoryFilter = 'all' | T.BackupActivityKind
type StatusFilter = 'all' | T.BackupRunState

const HISTORY_FILTERS: HistoryFilter[] = [
  'all',
  'manual',
  'automatic',
  'restore',
]
const STATUS_FILTERS: StatusFilter[] = [
  'all',
  'running',
  'succeeded',
  'partiallyFailed',
  'failed',
]

@Component({
  selector: 'backup-history',
  template: `
    <section class="history-toolbar">
      <tui-textfield class="history-search">
        <label tuiLabel>{{ 'Search backups' | i18n }}</label>
        <input
          tuiInput
          [ngModel]="query()"
          (ngModelChange)="setQuery($event)"
        />
      </tui-textfield>
      <tui-textfield
        tuiChevron
        [stringify]="stringifyFilter"
        [tuiTextfieldCleaner]="false"
      >
        <label tuiLabel>{{ 'Show' | i18n }}</label>
        <input
          tuiSelect
          [ngModel]="historyFilter()"
          (ngModelChange)="setHistoryFilter($event)"
        />
        <tui-data-list *tuiDropdown>
          @for (filter of historyFilters; track filter) {
            <button tuiOption [value]="filter">
              {{ stringifyFilter(filter) }}
            </button>
          }
        </tui-data-list>
      </tui-textfield>
      <tui-textfield
        tuiChevron
        [stringify]="stringifyStatusFilter"
        [tuiTextfieldCleaner]="false"
      >
        <label tuiLabel>{{ 'Status' | i18n }}</label>
        <input
          tuiSelect
          [ngModel]="statusFilter()"
          (ngModelChange)="setStatusFilter($event)"
        />
        <tui-data-list *tuiDropdown>
          @for (status of statusFilters; track status) {
            <button tuiOption [value]="status">
              {{ stringifyStatusFilter(status) }}
            </button>
          }
        </tui-data-list>
      </tui-textfield>
    </section>

    <section class="timeline">
      @for (activity of pagedActivities(); track activity.id) {
        <tui-accordion class="activity">
          <button tuiAccordion>
            <tui-icon [icon]="activityIcon(activity)" />
            <span tuiTitle>
              <b>{{ activityLabel(activity) }}</b>
              <span tuiSubtitle>
                {{ activity.startedAt | date: 'medium' }} ·
                {{ activityState(activity) }}
              </span>
            </span>
            <span tuiBadge [appearance]="activityAppearance(activity)">
              {{ activityState(activity) }}
            </span>
          </button>
          <tui-expand>
            <div class="activity-details">
              <p>
                <b>{{ 'Backup location' | i18n }}:</b>
                {{ backupService.targetName(activity.targetId) }}
              </p>
              <p>
                <b>
                  {{
                    (activity.intendedServices.length === 1
                      ? 'Service'
                      : 'Services'
                    ) | i18n
                  }}:
                </b>
                {{ activity.intendedServices.length }}
              </p>
              @if (
                activity.state === 'partiallyFailed' ||
                activity.state === 'failed'
              ) {
                <div
                  tuiNotification
                  [appearance]="activityAppearance(activity)"
                >
                  {{ failureSummary(activity) | i18n }}
                </div>
              }
              @for (
                report of activity.services | keyvalue: null;
                track report.key
              ) {
                <section class="service-report">
                  <b>{{ packageName(report.key) }}</b>
                  @if (report.value.error) {
                    <p class="g-negative">
                      {{ 'This service did not complete successfully.' | i18n }}
                    </p>
                  } @else {
                    <p>
                      {{ 'Completed' | i18n }} ·
                      {{ reportSize(report.value) }} · {{ 'Duration' | i18n }}:
                      {{ report.value.duration_ms / 1000 | number: '1.0-1' }}s
                    </p>
                  }
                  @for (
                    snapshot of snapshotsFor(activity, report.key);
                    track snapshot.id
                  ) {
                    <p class="checkpoint">
                      {{ 'Checkpoints' | i18n }}: {{ snapshot.id }} ·
                      {{ 'Version' | i18n }} {{ snapshot.packageVersion }}
                      @if (snapshot.archived) {
                        · {{ 'Archived' | i18n }}
                      }
                    </p>
                  }
                </section>
              }
              @if (technicalErrors(activity); as errors) {
                @if (errors.length) {
                  <tui-accordion class="technical-details">
                    <button tuiAccordion>
                      {{ 'Technical details' | i18n }}
                    </button>
                    <tui-expand>
                      @for (error of errors; track error.label) {
                        <p>
                          <b>{{ error.label }}:</b>
                          <span class="technical-error">
                            {{ error.detail }}
                          </span>
                        </p>
                      }
                    </tui-expand>
                  </tui-accordion>
                }
              }
            </div>
          </tui-expand>
        </tui-accordion>
      } @empty {
        <div tuiNotification appearance="info">
          {{ 'No backup activity yet.' | i18n }}
        </div>
      }
    </section>
    @if (pageCount() > 1) {
      <tui-pagination
        [length]="pageCount()"
        [index]="page()"
        [activePadding]="breakpoint() === 'mobile' ? 0 : 1"
        [sidePadding]="breakpoint() === 'mobile' ? 0 : 1"
        (indexChange)="setPage($event)"
      />
    }
  `,
  styles: `
    :host,
    .timeline {
      display: grid;
      gap: 0.75rem;
      inline-size: 100%;
      min-inline-size: 0;
    }

    :host {
      container-type: inline-size;
    }

    .history-toolbar {
      display: grid;
      grid-template-columns: minmax(0, 1fr) repeat(2, minmax(0, 14rem));
      gap: 0.75rem;
      min-inline-size: 0;
    }

    .history-toolbar tui-textfield {
      inline-size: 100%;
      min-inline-size: 0;
      max-inline-size: 100%;
      color: var(--tui-text-secondary);
      box-sizing: border-box;
    }

    tui-pagination {
      inline-size: 100%;
      min-inline-size: 0;
      max-inline-size: 100%;
    }

    .activity {
      padding: 0;
      overflow: hidden;
      inline-size: 100%;
      min-inline-size: 0;
      box-sizing: border-box;
    }

    .activity > button {
      display: flex;
      align-items: center;
      gap: 0.75rem;
      min-block-size: 3.5rem;
      block-size: auto;
      padding: 1rem 1.25rem;
      cursor: pointer;
      list-style: none;
      inline-size: 100%;
      min-inline-size: 0;
      max-inline-size: 100%;
      box-sizing: border-box;
      text-align: start;
    }

    .activity > button > *,
    .activity > button [tuiTitle] {
      min-inline-size: 0;
      overflow-wrap: anywhere;
    }

    .activity > button [tuiTitle] {
      flex: 1;
      white-space: normal;
    }

    [tuiSubtitle] {
      display: block;
      margin-block-start: 0.25rem;
      white-space: normal;
    }

    .activity-details {
      min-inline-size: 0;
      padding-block: 0 1rem;
      padding-inline: 3.25rem 1.25rem;
    }

    .activity-details p {
      margin-block: 0.35rem;
      margin-inline: 0;
    }

    .technical-details {
      margin-block-start: 0.75rem;
    }

    .technical-error {
      display: block;
      overflow-wrap: anywhere;
      white-space: pre-wrap;
    }

    .service-report {
      display: grid;
      gap: 0.25rem;
      padding-block: 0.75rem;
      border-block-start: 1px solid var(--tui-border-normal);
    }

    .service-report p {
      margin: 0;
      overflow-wrap: anywhere;
    }

    .checkpoint {
      color: var(--tui-text-secondary);
    }

    @container (max-inline-size: 42rem) {
      .history-search {
        grid-column: 1 / -1;
      }

      .history-toolbar {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }
    }

    @container (max-inline-size: 30rem) {
      .history-toolbar {
        grid-template-columns: 1fr;
      }

      .history-search {
        grid-column: auto;
      }

      .history-toolbar tui-textfield {
        inline-size: 100%;
      }

      .activity > button {
        align-items: flex-start;
        flex-direction: column;
      }

      .activity > button > tui-icon,
      .activity > button > [tuiBadge] {
        align-self: flex-start;
      }

      .activity-details {
        padding-inline: 0;
      }
    }
  `,
  host: { class: 'g-wrap-content' },
  imports: [
    DatePipe,
    DecimalPipe,
    KeyValuePipe,
    FormsModule,
    TuiBadge,
    TuiAccordion,
    TuiChevron,
    TuiDataList,
    TuiIcon,
    TuiInput,
    TuiLabel,
    TuiNotification,
    TuiTitle,
    TuiSelect,
    TuiPagination,
    i18nPipe,
  ],
})
export class BackupHistory {
  protected readonly breakpoint = inject(TUI_BREAKPOINT)
  protected readonly backupService = inject(BackupService)
  private readonly i18n = inject(i18nPipe)
  private readonly route = inject(ActivatedRoute)
  private readonly router = inject(Router)
  private readonly state = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('scheduledBackups'),
  )
  private readonly packageData = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('packageData'),
  )

  protected readonly historyFilter = signal<HistoryFilter>('all')
  protected readonly statusFilter = signal<StatusFilter>('all')
  protected readonly query = signal('')
  protected readonly page = signal(0)
  private readonly pageSize = 20
  private readonly activityStatuses = {
    running: { label: 'In progress', appearance: 'neutral' },
    succeeded: { label: 'Succeeded', appearance: 'positive' },
    partiallyFailed: { label: 'Partially failed', appearance: 'warning' },
    failed: { label: 'Failed', appearance: 'negative' },
  } as const satisfies Record<
    T.BackupRunState,
    { label: string; appearance: string }
  >
  protected readonly historyFilters = HISTORY_FILTERS
  protected readonly stringifyFilter = (filter: HistoryFilter) =>
    this.i18n.transform(
      filter === 'all'
        ? 'All activity'
        : filter === 'manual'
          ? 'Manual'
          : filter === 'automatic'
            ? 'Automatic'
            : 'Restore',
    )
  protected readonly statusFilters = STATUS_FILTERS
  protected readonly stringifyStatusFilter = (status: StatusFilter) =>
    this.i18n.transform(
      status === 'all' ? 'All statuses' : this.activityStatuses[status].label,
    )
  protected readonly activities = computed(() =>
    Object.values(this.state()?.activities || {}).sort((a, b) =>
      b.startedAt.localeCompare(a.startedAt),
    ),
  )
  private readonly snapshotsByTarget = computed(() => {
    const targets = new Map<
      string,
      Map<string, Map<string, T.ServiceSnapshot[]>>
    >()
    for (const history of Object.values(this.state()?.histories || {})) {
      let packages = targets.get(history.targetId)
      if (!packages) targets.set(history.targetId, (packages = new Map()))
      let runs = packages.get(history.packageId)
      if (!runs) packages.set(history.packageId, (runs = new Map()))
      for (const snapshot of history.snapshots) {
        const snapshots = runs.get(snapshot.runId)
        if (snapshots) snapshots.push(snapshot)
        else runs.set(snapshot.runId, [snapshot])
      }
    }
    return targets
  })

  protected readonly filteredActivities = computed(() => {
    const query = this.query().trim().toLocaleLowerCase()
    return this.activities().filter(activity => {
      const kindMatches =
        this.historyFilter() === 'all' || activity.kind === this.historyFilter()
      const statusMatches =
        this.statusFilter() === 'all' || activity.state === this.statusFilter()
      return kindMatches && statusMatches && this.matchesQuery(activity, query)
    })
  })
  protected readonly pageCount = computed(() =>
    Math.ceil(this.filteredActivities().length / this.pageSize),
  )
  protected readonly pagedActivities = computed(() => {
    const page = Math.min(this.page(), Math.max(0, this.pageCount() - 1))
    return this.filteredActivities().slice(
      page * this.pageSize,
      (page + 1) * this.pageSize,
    )
  })
  private readonly queryParamsSubscription = this.route.queryParamMap
    .pipe(
      takeUntilDestroyed(),
      tap(params => {
        const historyFilter = params.get('historyKind')
        const statusFilter = params.get('historyStatus')
        const page = Number(params.get('historyPage')) - 1
        this.query.set(params.get('historySearch') || '')
        this.historyFilter.set(
          HISTORY_FILTERS.includes(historyFilter as HistoryFilter)
            ? (historyFilter as HistoryFilter)
            : 'all',
        )
        this.statusFilter.set(
          STATUS_FILTERS.includes(statusFilter as StatusFilter)
            ? (statusFilter as StatusFilter)
            : 'all',
        )
        this.page.set(Number.isInteger(page) && page >= 0 ? page : 0)
      }),
    )
    .subscribe()

  constructor() {
    void this.backupService.getBackupTargets()
  }

  protected setQuery(query: string) {
    this.query.set(query)
    this.updateQueryParams({ historySearch: query || null, historyPage: null })
  }

  protected setHistoryFilter(filter: HistoryFilter) {
    this.historyFilter.set(filter)
    this.updateQueryParams({
      historyKind: filter === 'all' ? null : filter,
      historyPage: null,
    })
  }

  protected setStatusFilter(filter: StatusFilter) {
    this.statusFilter.set(filter)
    this.updateQueryParams({
      historyStatus: filter === 'all' ? null : filter,
      historyPage: null,
    })
  }

  protected setPage(page: number) {
    this.page.set(page)
    this.updateQueryParams({ historyPage: page ? page + 1 : null })
  }

  private updateQueryParams(
    queryParams: Record<string, string | number | null>,
  ) {
    void this.router.navigate([], {
      relativeTo: this.route,
      queryParams,
      queryParamsHandling: 'merge',
      replaceUrl: true,
    })
  }

  protected activityLabel(activity: T.BackupActivity): string {
    if (activity.kind === 'manual') return this.i18n.transform('Manual backup')
    if (activity.kind === 'restore') return this.i18n.transform('Restore')
    return activity.jobName || this.i18n.transform('Automatic backup')
  }

  protected activityState(activity: T.BackupActivity): string {
    return this.i18n.transform(this.activityStatuses[activity.state].label)
  }

  protected failureSummary(activity: T.BackupActivity): string {
    if (activity.state === 'partiallyFailed') {
      return 'Some services did not complete successfully. Review the affected services and try again.'
    }
    return activity.kind === 'restore'
      ? 'The restore did not complete. Check the password and backup location, then try again.'
      : 'The backup did not complete. Check the password and backup location, then try again.'
  }

  protected technicalErrors(
    activity: T.BackupActivity,
  ): { label: string; detail: string }[] {
    return [
      ...(activity.error
        ? [{ label: this.i18n.transform('Operation'), detail: activity.error }]
        : []),
      ...Object.entries(activity.services).flatMap(([packageId, report]) =>
        report.error
          ? [
              {
                label: this.packageName(packageId),
                detail: report.error,
              },
            ]
          : [],
      ),
    ]
  }

  protected activityIcon(activity: T.BackupActivity): string {
    if (activity.kind === 'manual') return '@tui.copy-plus'
    if (activity.kind === 'restore') return '@tui.database-backup'
    return '@tui.calendar-clock'
  }

  protected activityAppearance(
    activity: T.BackupActivity,
  ): 'positive' | 'warning' | 'negative' | 'neutral' {
    return this.activityStatuses[activity.state].appearance
  }

  protected snapshotsFor(
    activity: T.BackupActivity,
    packageId: string,
  ): T.ServiceSnapshot[] {
    return (
      this.snapshotsByTarget()
        .get(activity.targetId)
        ?.get(packageId)
        ?.get(activity.id) || []
    )
  }

  protected reportSize(report: T.PackageBackupReport): string {
    const bytes = report.physical_size ?? report.logical_size
    return bytes === null ? '—' : convertBytes(bytes)
  }

  protected packageName(id: string): string {
    if (id === SYSTEM_PACKAGE_ID) return this.i18n.transform('System')
    const pkg = this.packageData()?.[id]
    return (pkg && getManifest(pkg)?.title) || id
  }

  private matchesQuery(activity: T.BackupActivity, query: string): boolean {
    if (!query) return true
    const terms = [
      activity.startedAt,
      new Date(activity.startedAt).toLocaleString(),
      this.activityState(activity),
      this.stringifyFilter(activity.kind),
      this.activityLabel(activity),
      this.backupService.targetName(activity.targetId),
      ...activity.intendedServices.flatMap(id => [id, this.packageName(id)]),
    ]
    return terms.some(term => term.toLocaleLowerCase().includes(query))
  }
}
