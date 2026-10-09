import {
  afterNextRender,
  Component,
  computed,
  effect,
  ElementRef,
  inject,
  Injector,
  signal,
  viewChild,
} from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { ActivatedRoute, Router } from '@angular/router'
import { DocsLinkDirective, i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiAppearance,
  TuiButton,
  TuiCell,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiLink,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiBadge, TuiSwitch } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { OSService } from 'src/app/services/os.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { BackupService } from '../system/routes/backups/backup.service'
import SystemBackupComponent from '../system/routes/backups/backups.component'
import { DeleteScheduleService } from '../system/routes/backups/delete-schedule'
import { BackupProgressComponent } from '../system/routes/backups/progress.component'
import {
  backupJobNeedsAttention,
  formatBackupScheduleSummary,
  parseBackupSchedule,
} from '../system/routes/backups/scheduled-utils'
import AutomaticBackups from './automatic'
import { BackupHistory } from './history'
import BackupLocations from './locations'

const BACKUP_PANELS = [
  'automatic',
  'manual',
  'restore',
  'locations',
  'history',
] as const
type BackupPanel = (typeof BACKUP_PANELS)[number]

@Component({
  template: `
    <span *title>{{ 'Backups' | i18n }}</span>

    <header class="page-heading">
      <div>
        <h2>
          {{ 'Backups' | i18n }}
          <a
            tuiIconButton
            docsLink
            appearance="icon"
            fragment="#backups"
            iconStart="@tui.book-open-text"
            path="/start-os/"
            size="xs"
            [attr.aria-label]="'Documentation' | i18n"
          ></a>
        </h2>
        <p>
          {{
            'Protect your services automatically, create a manual backup, or restore from an earlier checkpoint.'
              | i18n
          }}
        </p>
      </div>
    </header>

    @if (operationActivity(); as activity) {
      @if (manualRunning()) {
        <button
          #progressCard
          backupProgress
          type="button"
          class="progress-prominent"
          (click)="goToServices()"
        ></button>
      } @else {
        <button
          #progressCard
          type="button"
          class="operation"
          tuiCell
          (click)="goToServices()"
        >
          <tui-icon class="g-primary" icon="@tui.loader-circle" />
          <span tuiTitle>
            <b>{{ operationTitle(activity) | i18n }}</b>
            <span tuiSubtitle>
              {{ 'You can leave this page. Progress will continue.' | i18n }}
            </span>
          </span>
          <span tuiBadge appearance="info">{{ 'In progress' | i18n }}</span>
        </button>
      }
    }

    <section
      tuiCardLarge="compact"
      appearance="secondary-grayscale"
      class="backup-card"
    >
      <header
        tuiHeader
        class="card-heading automatic-heading"
        [class._single-job]="jobs().length === 1"
      >
        <button
          tuiCell
          type="button"
          class="card-toggle"
          [attr.aria-expanded]="expanded() === 'automatic'"
          (click)="togglePanel('automatic')"
        >
          <tui-icon
            [icon]="
              needsAttention() ? '@tui.triangle-alert' : '@tui.calendar-clock'
            "
          />
          <span tuiTitle>
            <b>
              {{
                (needsAttention()
                  ? 'Automatic backups need attention'
                  : 'Automatic backups'
                ) | i18n
              }}
            </b>
            <span tuiSubtitle>
              @if (needsAttention()) {
                {{ healthDetail() | i18n }}
              } @else {
                {{ automaticSummary() }}
              }
            </span>
          </span>
        </button>

        <span tuiAccessories class="card-accessories">
          @if (needsAttention()) {
            <button tuiLink type="button" (click)="openHistory()">
              {{ 'See more' | i18n }}
            </button>
          }

          @if (jobs().length === 1) {
            <span class="automatic-controls">
              @if (!primary()?.enabled) {
                <span tuiBadge>{{ 'Paused' | i18n }}</span>
              }
              <label class="simple-switch">
                <input
                  tuiSwitch
                  type="checkbox"
                  [attr.aria-label]="'Automatic backups' | i18n"
                  [ngModel]="primary()?.enabled ?? false"
                  [disabled]="changingAutomatic()"
                  (ngModelChange)="setAutomatic($event)"
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
                    [disabled]="!canRunNow()"
                    (click)="runNow()"
                  >
                    {{ 'Run now' | i18n }}
                  </button>
                  <button tuiOption (click)="openAutomaticEditor()">
                    {{ 'View/Edit' | i18n }}
                  </button>
                  <button tuiOption (click)="addSchedule()">
                    {{ 'Add schedule' | i18n }}
                  </button>
                  <button
                    tuiOption
                    tuiAppearance="flat-destructive"
                    (click)="deleteSchedule()"
                  >
                    {{ 'Delete schedule' | i18n }}
                  </button>
                </tui-data-list>
              </button>
            </span>
          } @else {
            <button
              tuiIconButton
              type="button"
              size="m"
              appearance="flat-grayscale"
              [iconStart]="
                expanded() === 'automatic'
                  ? '@tui.chevron-up'
                  : '@tui.chevron-down'
              "
              [attr.aria-expanded]="expanded() === 'automatic'"
              (click)="togglePanel('automatic')"
            >
              {{
                (expanded() === 'automatic'
                  ? 'Collapse automatic backups'
                  : 'Expand automatic backups'
                ) | i18n
              }}
            </button>
          }
        </span>
      </header>

      @if (expanded() === 'automatic') {
        <div class="card-body">
          <automatic-backups
            [embedded]="true"
            [mode]="jobs().length ? 'manage' : 'setup'"
            [createRequest]="createScheduleRequest()"
            [reviewPackageId]="reviewPackageId"
            (manageLocations)="openLocations()"
            (createRequestHandled)="createScheduleRequest.set(false)"
            (collapseRequested)="collapseAutomatic($event)"
          />
        </div>
      }
    </section>

    <section
      tuiCardLarge="compact"
      appearance="secondary-grayscale"
      class="backup-card"
    >
      <header tuiHeader class="card-heading">
        <button
          tuiCell
          type="button"
          class="card-toggle"
          [attr.aria-expanded]="expanded() === 'manual'"
          (click)="togglePanel('manual')"
        >
          <tui-icon icon="@tui.copy-plus" />
          <span tuiTitle>
            <b>{{ 'Create a manual backup' | i18n }}</b>
            <span tuiSubtitle>{{ 'Run a one-time backup now' | i18n }}</span>
          </span>
          <tui-icon
            icon="@tui.chevron-down"
            [class._rotated]="expanded() === 'manual'"
          />
        </button>
      </header>
      @if (expanded() === 'manual') {
        <div class="card-body">
          <system-backup
            mode="create"
            [embedded]="true"
            [operationActive]="progressActive()"
            (manageLocations)="openLocations()"
          />
        </div>
      }
    </section>

    <section
      tuiCardLarge="compact"
      appearance="secondary-grayscale"
      class="backup-card"
    >
      <header tuiHeader class="card-heading">
        <button
          tuiCell
          type="button"
          class="card-toggle"
          [attr.aria-expanded]="expanded() === 'restore'"
          (click)="togglePanel('restore')"
        >
          <tui-icon icon="@tui.database-backup" />
          <span tuiTitle>
            <b>{{ 'Restore from a backup' | i18n }}</b>
            <span tuiSubtitle>
              {{ 'Choose a manual or automatic checkpoint' | i18n }}
            </span>
          </span>
          <tui-icon
            icon="@tui.chevron-down"
            [class._rotated]="expanded() === 'restore'"
          />
        </button>
      </header>
      @if (expanded() === 'restore') {
        <div class="card-body">
          <system-backup
            mode="restore"
            [embedded]="true"
            [operationActive]="progressActive()"
            (manageLocations)="openLocations()"
          />
        </div>
      }
    </section>

    <section
      tuiCardLarge="compact"
      appearance="secondary-grayscale"
      class="backup-card"
    >
      <header tuiHeader class="card-heading">
        <button
          tuiCell
          type="button"
          class="card-toggle"
          [attr.aria-expanded]="expanded() === 'locations'"
          (click)="togglePanel('locations')"
        >
          <tui-icon icon="@tui.hard-drive" />
          <span tuiTitle>
            <b>{{ 'Manage backup locations' | i18n }}</b>
            <span tuiSubtitle>
              {{ 'Add or repair a physical drive or network folder' | i18n }}
            </span>
          </span>
          <tui-icon
            icon="@tui.chevron-down"
            [class._rotated]="expanded() === 'locations'"
          />
        </button>
      </header>
      @if (expanded() === 'locations') {
        <div class="card-body">
          <backup-locations [embedded]="true" />
        </div>
      }
    </section>

    <section
      #historyCard
      tuiCardLarge="compact"
      appearance="secondary-grayscale"
      class="backup-card"
    >
      <header tuiHeader class="card-heading">
        <button
          tuiCell
          type="button"
          class="card-toggle"
          [attr.aria-expanded]="expanded() === 'history'"
          (click)="togglePanel('history')"
        >
          <tui-icon icon="@tui.history" />
          <span tuiTitle>
            <b>{{ 'Backup history' | i18n }}</b>
            <span tuiSubtitle>
              {{ activities().length }} {{ 'All activity' | i18n }}
            </span>
          </span>
          <tui-icon
            icon="@tui.chevron-down"
            [class._rotated]="expanded() === 'history'"
          />
        </button>
      </header>
      @if (expanded() === 'history') {
        <div class="card-body">
          <backup-history />
        </div>
      }
    </section>
  `,
  styles: `
    :host {
      display: grid;
      gap: 0.75rem;
      inline-size: 100%;
      min-inline-size: 0;
      max-inline-size: 64rem;
      margin-inline: auto;
      container-type: inline-size;
    }

    h2,
    p {
      margin: 0;
    }

    h2 {
      display: flex;
      align-items: center;
      gap: 0.25rem;
    }

    .page-heading p,
    [tuiSubtitle] {
      display: block;
      margin-block-start: 0.25rem;
      color: var(--tui-text-secondary);
    }

    [tuiCardLarge].backup-card {
      overflow: hidden;
      container: card / inline-size;
    }

    .card-heading {
      min-inline-size: 0;
    }

    .card-toggle {
      flex: 1;
      inline-size: 100%;
      min-inline-size: 0;
      text-align: start;
      cursor: pointer;
    }

    .card-toggle:disabled {
      cursor: default;
      opacity: var(--tui-disabled-opacity);
    }

    .card-toggle [tuiTitle] {
      flex: 1;
      min-inline-size: 0;
      overflow-wrap: anywhere;
    }

    .card-toggle > tui-icon:last-child {
      transition: transform var(--tui-duration, 0.2s);
    }

    ._rotated {
      transform: rotate(180deg);
    }

    .card-accessories,
    .automatic-controls,
    .simple-switch {
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }

    .card-heading .card-accessories {
      align-items: center;
      flex-wrap: wrap;
      justify-content: flex-end;
      gap: 1rem;
    }

    .automatic-controls {
      flex: none;
      margin-inline-start: auto;
      gap: 1rem;
    }

    .simple-switch {
      inline-size: fit-content;
      white-space: normal;
    }

    .card-body {
      display: grid;
      gap: 1rem;
      min-inline-size: 0;
      padding-block-start: 1rem;
      border-block-start: 1px solid var(--tui-border-normal);
    }

    .automatic-heading + .card-body {
      border-block-start: 0;
    }

    .operation,
    .attention {
      gap: 0.75rem;
      min-inline-size: 0;
    }

    .operation {
      position: static;
      z-index: 1;
      inline-size: 100%;
      color: inherit;
      font: inherit;
      background: color-mix(in hsl, var(--start9-base-1) 50%, transparent);
      border: 1px solid var(--tui-border-normal);
      border-radius: var(--tui-radius-l);
      box-sizing: border-box;
      cursor: pointer;
    }

    .operation > tui-icon {
      animation: backup-progress-spin 1.5s linear infinite;
    }

    .operation [tuiTitle],
    .attention [tuiTitle] {
      flex: 1;
      min-inline-size: 0;
      overflow-wrap: anywhere;
    }

    .progress-prominent {
      position: static;
      z-index: 1;
      display: grid;
      inline-size: 100%;
      padding: 0.75rem;
      color: inherit;
      font: inherit;
      text-align: start;
      background: color-mix(in hsl, var(--start9-base-1) 50%, transparent);
      border: 1px solid var(--tui-border-normal);
      border-radius: var(--tui-radius-l);
      box-sizing: border-box;
      cursor: pointer;
    }

    .operation:hover,
    .progress-prominent:hover {
      border-color: var(--tui-border-hover);
    }

    @keyframes backup-progress-spin {
      to {
        transform: rotate(1turn);
      }
    }

    @container card (max-inline-size: 44rem) {
      .card-heading {
        align-items: stretch;
        flex-direction: column;
      }

      .card-accessories {
        inline-size: 100%;
      }

      .automatic-heading._single-job .card-toggle b {
        white-space: normal;
      }
    }

    @container (max-inline-size: 30rem) {
      .card-toggle {
        align-items: flex-start;
      }

      .operation {
        align-items: stretch;
        flex-direction: column;
      }

      .operation > tui-icon,
      .operation > [tuiBadge] {
        align-self: flex-start;
      }
    }
  `,
  host: { class: 'g-wrap-content' },
  imports: [
    FormsModule,
    TuiAppearance,
    TuiBadge,
    TuiButton,
    TuiCardLarge,
    TuiCell,
    TuiDataList,
    TuiDropdown,
    TuiHeader,
    TuiIcon,
    TuiLink,
    TuiSwitch,
    TuiTitle,
    TitleDirective,
    AutomaticBackups,
    SystemBackupComponent,
    BackupLocations,
    BackupHistory,
    BackupProgressComponent,
    DocsLinkDirective,
    i18nPipe,
  ],
})
export default class BackupsComponent {
  private readonly automatic = viewChild(AutomaticBackups)
  private readonly historyCard =
    viewChild<ElementRef<HTMLElement>>('historyCard')
  private readonly progressCard =
    viewChild<ElementRef<HTMLElement>>('progressCard')
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly backupService = inject(BackupService)
  private readonly deleteScheduleService = inject(DeleteScheduleService)
  private readonly i18n = inject(i18nPipe)
  private readonly os = inject(OSService)
  private readonly router = inject(Router)
  private readonly route = inject(ActivatedRoute)
  private readonly queryParams = toSignal(this.route.queryParamMap, {
    initialValue: this.route.snapshot.queryParamMap,
  })
  private readonly injector = inject(Injector)
  private readonly state = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('scheduledBackups'),
  )

  protected readonly reviewPackageId =
    this.route.snapshot.queryParamMap.get('addService') || ''
  protected readonly expanded = computed<BackupPanel | null>(() => {
    const panel = this.queryParams().get('panel')
    return panel === 'overview'
      ? null
      : (BACKUP_PANELS.find(value => value === panel) ??
          (this.reviewPackageId
            ? 'automatic'
            : this.route.snapshot.data['panel']))
  })
  private readonly progressRequest = signal<{
    jobId: string
    previousActivityId: string | null
  } | null>(null)
  protected readonly createScheduleRequest = signal(
    this.route.snapshot.queryParamMap.has('createSchedule'),
  )
  protected readonly manualRunning = toSignal(this.os.backingUp$, {
    initialValue: false,
  })
  protected readonly changingAutomatic = signal(false)

  constructor() {
    void this.backupService.getBackupTargets()
    effect(() => {
      const request = this.progressRequest()
      const activity = this.activities()[0]
      if (!request || !activity || activity.id === request.previousActivityId) {
        return
      }
      if (activity.jobId !== request.jobId || activity.state !== 'running') {
        this.progressRequest.set(null)
        return
      }
      afterNextRender(
        () => {
          this.progressCard()?.nativeElement.scrollIntoView({
            behavior: 'smooth',
            block: 'start',
          })
          this.progressRequest.set(null)
        },
        { injector: this.injector },
      )
    })
  }

  protected readonly jobs = computed(() =>
    Object.values(this.state()?.jobs || {}).sort((a, b) =>
      a.createdAt.localeCompare(b.createdAt),
    ),
  )
  protected readonly primary = computed(() => this.jobs()[0])
  protected readonly activities = computed(() =>
    Object.values(this.state()?.activities || {}).sort((a, b) =>
      b.startedAt.localeCompare(a.startedAt),
    ),
  )
  protected readonly operationActivity = computed(() => {
    const latest = this.activities()[0]
    return latest?.state === 'running' ? latest : null
  })
  protected readonly progressActive = computed(() => !!this.operationActivity())
  protected readonly automaticOn = computed(() =>
    this.jobs().some(job => job.enabled && !job.pause),
  )
  protected readonly needsAttention = computed(() =>
    this.jobs().some(backupJobNeedsAttention),
  )

  protected async togglePanel(panel: BackupPanel) {
    if (
      this.expanded() === 'automatic' &&
      !((await this.automatic()?.confirmDiscardChanges()) ?? true)
    ) {
      return
    }
    await this.setPanel(this.expanded() === panel ? null : panel)
  }

  protected async openLocations() {
    if (!((await this.automatic()?.confirmDiscardChanges()) ?? true)) return
    await this.setPanel('locations')
  }

  protected async openHistory() {
    if (!((await this.automatic()?.confirmDiscardChanges()) ?? true)) return
    if (!(await this.setPanel('history'))) return
    afterNextRender(
      () =>
        this.historyCard()?.nativeElement.scrollIntoView({
          behavior: 'smooth',
          block: 'start',
        }),
      { injector: this.injector },
    )
  }

  protected async collapseAutomatic(runNowJobId: string | null) {
    if (!((await this.automatic()?.confirmDiscardChanges()) ?? true)) return
    if (!(await this.setPanel(null))) return
    this.progressRequest.set(
      runNowJobId && !this.operationActivity()
        ? {
            jobId: runNowJobId,
            previousActivityId: this.activities()[0]?.id || null,
          }
        : null,
    )
  }

  protected openAutomaticEditor() {
    return this.setPanel('automatic')
  }

  protected async addSchedule() {
    if (!(await this.setPanel('automatic'))) return
    this.createScheduleRequest.set(true)
  }

  private async setPanel(panel: BackupPanel | null) {
    if (this.expanded() === panel) return true
    return this.router.navigate([], {
      relativeTo: this.route,
      queryParams: { panel: panel ?? 'overview' },
      queryParamsHandling: 'merge',
    })
  }

  protected async goToServices() {
    if (!(await this.canDeactivate())) return
    await this.router.navigate(['/services'])
  }

  async canDeactivate(): Promise<boolean> {
    return (await this.automatic()?.confirmDiscardChanges()) ?? true
  }

  protected automaticSummary(): string {
    const jobs = this.jobs()
    if (!jobs.length) {
      return this.i18n.transform('Automatic backups are not set up yet.')
    }
    if (jobs.length > 1) {
      const summary = `${jobs.length} · ${this.i18n.transform('Automatic schedules')}`
      return this.automaticOn()
        ? summary
        : `${this.i18n.transform('Off')} · ${summary}`
    }
    const primary = jobs[0]!
    const schedule = parseBackupSchedule(primary.schedule)
    const timing = formatBackupScheduleSummary(schedule, label =>
      this.i18n.transform(label),
    )
    return this.automaticOn()
      ? timing
      : `${this.i18n.transform('Off')} · ${timing}`
  }

  protected healthDetail(): string {
    const job = this.jobs().find(backupJobNeedsAttention)
    if (job?.pause?.reason === 'reauthenticationRequired') {
      return 'The backup location needs your password again.'
    }
    if (job?.pause?.reason === 'targetIdentityMismatch') {
      return 'The connected backup location is not the expected location.'
    }
    if (job?.pause?.reason === 'targetUnreadable') {
      return 'StartOS could not read the backup location. Check Backup history for details.'
    }
    if (job?.pause?.reason === 'targetUnavailable') {
      return 'StartOS cannot connect to the backup location.'
    }
    return 'The latest automatic backup did not finish successfully.'
  }

  protected canRunNow(): boolean {
    const primary = this.primary()
    return (
      !!primary && primary.enabled && !primary.pause && !this.progressActive()
    )
  }

  protected operationTitle(activity: T.BackupActivity): string {
    if (activity.kind === 'restore') return 'Restoring services'
    if (activity.kind === 'manual') return 'Creating manual backup'
    return 'Creating automatic backup'
  }

  protected async runNow() {
    const job = this.primary()
    if (!job) return
    await this.tasks.run(
      () => this.api.runScheduledBackupJob({ id: job.id }),
      'Creating automatic backup',
    )
  }

  protected async deleteSchedule() {
    const job = this.primary()
    if (!job) return
    await this.deleteScheduleService.delete(job)
  }

  protected async setAutomatic(enabled: boolean) {
    if (enabled === this.jobs().every(job => job.enabled)) return
    this.changingAutomatic.set(true)
    await this.tasks.run(
      () =>
        this.api.setScheduledBackupJobsEnabled({
          ids: this.jobs().map(job => job.id),
          enabled,
        }),
      enabled ? 'Resume all' : 'Pause all',
    )
    this.changingAutomatic.set(false)
  }
}
