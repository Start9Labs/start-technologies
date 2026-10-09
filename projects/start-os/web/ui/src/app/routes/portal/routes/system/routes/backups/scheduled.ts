import { DatePipe } from '@angular/common'
import {
  afterNextRender,
  Component,
  computed,
  DestroyRef,
  Directive,
  effect,
  ElementRef,
  inject,
  Injector,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import {
  AbstractControl,
  FormsModule,
  NonNullableFormBuilder,
  NgControl,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import {
  convertBytes,
  DialogService,
  ErrorService,
  getErrorMessage,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TUI_VALIDATION_ERRORS,
  TuiAppearance,
  TuiButton,
  TuiCell,
  TuiCheckbox,
  TuiDataList,
  TuiDropdown,
  TuiError,
  TuiGroup,
  TuiIcon,
  TuiInput,
  TuiLabel,
  TuiLoader,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import {
  TuiAccordion,
  TuiBadge,
  TuiBlock,
  TuiChevron,
  TuiSelect,
  TuiSwitch,
} from '@taiga-ui/kit'
import { TuiCardLarge, TuiForm, TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { filter, firstValueFrom, from, map, race } from 'rxjs'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getManifest } from 'src/app/utils/get-package-data'
import { BackupService } from './backup.service'
import { DeleteScheduleService } from './delete-schedule'
import { BACKUP_RETENTION_CONFIRM } from './retention-confirm'
import { BackupRetentionRules } from './retention-rules'
import { BackupScheduleBrowser } from './schedule-browser'
import { BackupScheduleControls } from './schedule-controls'
import { BackupScheduleEditor } from './schedule-editor'
import {
  backupPauseLabel,
  BackupRetentionRuleValue,
  BackupRetentionTierEditor,
  BackupScheduleFormValue,
  BackupServiceSelection,
  formatBackupRetentionRule,
  formatBackupScheduleSummary,
  formatBackupServiceSummary,
  hasDuplicateRetentionRules,
  isValidBackupRetentionRules,
  isValidBackupSchedule,
  parseBackupRetentionTier,
  parseBackupSchedule,
  parseBackupServiceSelection,
  removeBackupRetentionRule,
  serializeBackupRetentionPolicy,
  serializeBackupSchedule,
  serializeBackupServiceSelection,
  SYSTEM_PACKAGE_ID,
} from './scheduled-utils'

interface RetentionOverrideEditor {
  tiers: BackupRetentionTierEditor[]
}

interface ConfirmedRetentionChange {
  history: T.ServiceTargetHistory
  policy: T.RetentionPolicy
  preview: T.RetentionPolicyChangePreview
}

interface JobEditorValue
  extends
    BackupRetentionTierEditor,
    BackupScheduleFormValue,
    BackupServiceSelection {
  id?: string
  name: string
  targetId: string
  keepAdditional: boolean
  additionalTiers: BackupRetentionTierEditor[]
  retentionOverrides: Record<string, RetentionOverrideEditor>
  password: string
  firstBackupNow: boolean
  capacityConfirmed: boolean
}

@Directive({
  selector: 'input[backupJobName]',
  host: { '(blur)': 'normalize()', '(keydown.enter)': 'normalize()' },
})
class BackupJobName {
  private readonly control = inject(NgControl)

  protected normalize() {
    const control = this.control.control
    if (control) control.setValue(control.value.trim())
  }
}

class JobEditor
  extends BackupScheduleEditor
  implements
    BackupRetentionTierEditor,
    BackupScheduleFormValue,
    BackupServiceSelection
{
  id?: string
  packageIds: string[]
  preservedSelectedPackageIds: string[]
  preservedExcludedPackageIds: string[]
  interval: BackupRetentionTierEditor['interval']
  duration: number
  customIntervalHours: number
  customCoverageHours: number
  additionalTiers: BackupRetentionTierEditor[]
  retentionOverrides: Record<string, RetentionOverrideEditor>

  readonly form

  constructor(
    formBuilder: NonNullableFormBuilder,
    value: JobEditorValue,
    jobs: readonly T.BackupJob[],
  ) {
    super(value)
    this.id = value.id
    this.packageIds = value.packageIds
    this.preservedSelectedPackageIds = value.preservedSelectedPackageIds
    this.preservedExcludedPackageIds = value.preservedExcludedPackageIds
    this.interval = value.interval
    this.duration = value.duration
    this.customIntervalHours = value.customIntervalHours
    this.customCoverageHours = value.customCoverageHours
    this.additionalTiers = value.additionalTiers
    this.retentionOverrides = value.retentionOverrides
    this.form = formBuilder.group({
      name: [
        value.name,
        [
          Validators.required,
          (control: AbstractControl<string>) =>
            jobs.some(
              job =>
                job.id !== value.id && job.name.trim() === control.value.trim(),
            )
              ? { duplicateJobName: true }
              : null,
        ],
      ],
      targetId: [
        { value: value.targetId, disabled: !!value.id },
        Validators.required,
      ],
      includeFuture: [value.includeFuture],
      keepAdditional: [value.keepAdditional],
      password: [value.password, value.id ? [] : Validators.required],
      firstBackupNow: [value.firstBackupNow],
      capacityConfirmed: [value.capacityConfirmed],
    })
  }

  get name() {
    return this.form.controls.name.value
  }
  set name(value: string) {
    this.form.controls.name.setValue(value)
  }

  get targetId() {
    return this.form.controls.targetId.value
  }
  set targetId(value: string) {
    this.form.controls.targetId.setValue(value)
  }

  toJSON() {
    return {
      id: this.id,
      ...this.scheduleValue(),
      packageIds: this.packageIds,
      preservedSelectedPackageIds: this.preservedSelectedPackageIds,
      preservedExcludedPackageIds: this.preservedExcludedPackageIds,
      interval: this.interval,
      duration: this.duration,
      customIntervalHours: this.customIntervalHours,
      customCoverageHours: this.customCoverageHours,
      additionalTiers: this.additionalTiers,
      retentionOverrides: this.retentionOverrides,
      ...this.form.getRawValue(),
    }
  }
}

@Component({
  selector: 'section[scheduledBackups]',
  providers: [
    {
      provide: TUI_VALIDATION_ERRORS,
      useFactory: () => {
        const i18n = inject(i18nPipe)
        return {
          ...inject(TUI_VALIDATION_ERRORS, { skipSelf: true }),
          duplicateJobName: () =>
            i18n.transform(
              'Every backup schedule needs a unique name. Choose a different name.',
            ),
        }
      },
    },
  ],
  template: `
    @if (mode() !== 'manage' || (!loading() && jobs().length <= 1)) {
      <div tuiNotification appearance="info" icon="@tui.calendar-clock">
        {{
          'Automatic checkpoints are stored separately from your latest manual checkpoint.'
            | i18n
        }}
      </div>
    }

    @for (
      review of jobs().length > 1 && !editor() ? visibleReviews() : [];
      track review.packageId
    ) {
      <section class="review" tuiAppearance="floating">
        <div tuiTitle>
          <b>
            {{ 'Add to backup schedule' | i18n }} —
            {{ packageName(review.packageId) }}
          </b>
          <div tuiSubtitle>
            {{
              'Choose which automatic backup schedules should include this service.'
                | i18n
            }}
          </div>
        </div>
        <label tuiLabel class="checkbox-row toggle-all">
          <span tuiTitle>
            <b>{{ 'Toggle all' | i18n }}</b>
          </span>
          <input
            tuiCheckbox
            type="checkbox"
            size="s"
            [ngModel]="allReviewJobsSelected(review)"
            (ngModelChange)="setAllReviewJobs(review, $event)"
          />
        </label>
        @for (job of jobs(); track job.id) {
          <label tuiLabel class="checkbox-row review-job">
            <span tuiTitle>
              <b>{{ jobName(job.id) }}</b>
            </span>
            <input
              tuiCheckbox
              type="checkbox"
              size="s"
              [ngModel]="reviewDecision(review.packageId, job.id)"
              (ngModelChange)="
                setReviewDecision(review.packageId, job.id, $event)
              "
            />
          </label>
        }
        <footer class="review-actions">
          <button
            tuiButton
            type="button"
            size="s"
            appearance="flat"
            (click)="create(review)"
          >
            {{ 'Add new schedule' | i18n }}
          </button>
          <button tuiButton size="s" (click)="resolveReview(review)">
            {{ 'Save backup schedules' | i18n }}
          </button>
        </footer>
      </section>
    }

    @if (mode() === 'manage') {
      @if (loading()) {
        <tui-loader [textContent]="'Loading' | i18n" />
      }

      @if (jobs().length > 1 && editor()) {
        <button
          tuiCell
          type="button"
          class="view-all-jobs"
          (click)="viewAllJobs()"
        >
          <tui-icon icon="@tui.list" />
          <span tuiTitle>
            <b>{{ 'View all schedules' | i18n }}</b>
          </span>
          <tui-icon icon="@tui.chevron-left" />
        </button>
      } @else if (jobs().length && !editor()) {
        <backup-schedule-browser
          [jobs]="jobs()"
          [packageIds]="packageIds()"
          [targets]="backupService.targets()"
          (enabledChange)="setJobEnabled($event.job, $event.enabled)"
          (runRequested)="runNow($event)"
          (editRequested)="edit($event)"
          (deleteRequested)="deleteJob($event)"
          (createRequested)="create()"
        />
      }

      @if (editor(); as form) {
        <form
          tuiCardLarge
          tuiForm="m"
          appearance="floating"
          [formGroup]="form.form"
          (submit.prevent)="save(form)"
        >
          <header tuiHeader class="editor-heading">
            <span tuiTitle>
              @if (isDefaultJob(form)) {
                <b>{{ 'Edit automatic schedule' | i18n }}</b>
              } @else {
                <b>{{ form.name || ('Create automatic schedule' | i18n) }}</b>
                @if (form.id) {
                  <span tuiSubtitle>
                    {{ 'Edit automatic schedule' | i18n }}
                  </span>
                }
              }
            </span>
            <button
              tuiButton
              type="button"
              size="xs"
              appearance="primary"
              (click)="cancelEditor()"
            >
              {{ 'Cancel' | i18n }}
            </button>
          </header>

          @if (selectedJob(); as job) {
            <div class="selected-job">
              <span tuiTitle>
                <span tuiSubtitle>
                  {{ backupService.targetName(job.targetId) }} ·
                  {{ 'Next run' | i18n }}:
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
              <div class="actions">
                @if (job.pause && job.pause.reason !== 'user') {
                  <button
                    tuiButton
                    type="button"
                    size="xs"
                    appearance="primary"
                    (click)="retry(job)"
                  >
                    {{ 'Retry backup location' | i18n }}
                  </button>
                  <button
                    tuiButton
                    type="button"
                    size="xs"
                    appearance="primary"
                    (click)="beginReassign(job)"
                  >
                    {{ 'Change backup location' | i18n }}
                  </button>
                }
              </div>
            </div>
          }

          @if (!isDefaultJob(form)) {
            <div class="setting-row vertical">
              <span tuiTitle>
                <b>{{ 'Schedule name' | i18n }}</b>
              </span>
              <tui-textfield>
                <label tuiLabel>{{ 'Schedule name' | i18n }}</label>
                <input
                  #jobNameInput
                  tuiInput
                  backupJobName
                  formControlName="name"
                />
              </tui-textfield>
              <tui-error formControlName="name" />
            </div>
          }

          <div class="setting-row vertical">
            <span tuiTitle>
              <b>{{ 'Backup location' | i18n }}</b>
              <span tuiSubtitle>
                {{ backupService.targetName(form.targetId) }}
              </span>
            </span>
            <tui-textfield
              tuiChevron
              [stringify]="stringifyTarget"
              [tuiTextfieldCleaner]="false"
            >
              <label tuiLabel>{{ 'Backup location' | i18n }}</label>
              <input tuiSelect formControlName="targetId" />
              <tui-data-list *tuiDropdown>
                @for (target of backupService.targets(); track target.id) {
                  <button tuiOption [value]="target.id">
                    {{ target.name }}
                  </button>
                }
              </tui-data-list>
            </tui-textfield>
            <tui-error formControlName="targetId" />
          </div>

          <div class="setting-row vertical">
            <span tuiTitle>
              <b>{{ 'Schedule' | i18n }}</b>
              <span tuiSubtitle>{{ scheduleSummary(form) }}</span>
            </span>
            <backup-schedule-controls
              [schedule]="form"
              (scheduleChange)="updateSchedule(form, $event)"
            />
          </div>

          <div class="setting-row vertical services-setting">
            <tui-accordion class="g-wrap-accordion" size="s">
              <button
                [tuiAccordion]="showServices()"
                (tuiAccordionChange)="showServices.set(!!$event)"
              >
                <span tuiTitle>
                  <b>{{ 'Services' | i18n }}</b>
                  <span tuiSubtitle>{{ selectedServiceSummary(form) }}</span>
                </span>
              </button>
              <tui-expand>
                <div class="services-options">
                  <label class="checkbox-row include-future">
                    <input
                      tuiCheckbox
                      type="checkbox"
                      formControlName="includeFuture"
                    />
                    <span tuiTitle>
                      <b>
                        {{ 'Automatically include future services' | i18n }}
                      </b>
                      <span tuiSubtitle>
                        {{
                          'All current and future services are included unless you exclude them.'
                            | i18n
                        }}
                      </span>
                    </span>
                  </label>
                  <label class="checkbox-row toggle-all">
                    <input
                      tuiCheckbox
                      type="checkbox"
                      [ngModel]="allPackagesSelected(form)"
                      [ngModelOptions]="{ standalone: true }"
                      (ngModelChange)="setAllPackages(form, $event)"
                    />
                    <span tuiTitle>
                      <b>{{ 'Toggle all services' | i18n }}</b>
                    </span>
                  </label>
                  <div tuiGroup orientation="vertical" [collapsed]="true">
                    @for (pkg of packages(); track pkg.id) {
                      <label tuiBlock="m">
                        <input
                          tuiCheckbox
                          type="checkbox"
                          [ngModel]="form.packageIds.includes(pkg.id)"
                          [ngModelOptions]="{ standalone: true }"
                          (ngModelChange)="togglePackage(form, pkg.id, $event)"
                        />
                        @if (pkg.id === systemPackageId) {
                          <tui-icon icon="@tui.settings" />
                        } @else {
                          <img alt="" [src]="pkg.icon" />
                        }
                        <span tuiTitle>
                          <b>{{ pkg.name }}</b>
                        </span>
                      </label>
                    }
                  </div>
                </div>
              </tui-expand>
            </tui-accordion>
          </div>

          <div class="setting-row vertical retention-setting">
            <div class="retention-heading setting-row">
              <span tuiTitle>
                <b>{{ 'Version history' | i18n }}</b>
                <span tuiSubtitle>{{ retentionSummary(form) }}</span>
              </span>
              <label class="inline-switch">
                <span class="retention-toggle-label">
                  {{ 'Keep additional versions' | i18n }}
                </span>
                <input
                  tuiSwitch
                  type="checkbox"
                  [attr.aria-label]="'Keep additional versions' | i18n"
                  formControlName="keepAdditional"
                />
              </label>
            </div>
            @if (form.keepAdditional) {
              <backup-retention-rules
                [rules]="retentionRules(form)"
                (ruleChange)="
                  updateRetentionRule(form, $event.index, $event.value)
                "
                (addRequested)="addRetentionRule(form)"
                (removeRequested)="removeRetentionRule(form, $event)"
              />
              @if (retentionHasDuplicates(form)) {
                <div tuiNotification appearance="negative">
                  {{ 'Each version-history rule must be unique.' | i18n }}
                </div>
              }
            }
          </div>

          <fieldset>
            <div class="heading estimate-heading">
              <legend>{{ 'Capacity estimates' | i18n }}</legend>
              <button
                tuiButton
                type="button"
                size="xs"
                appearance="primary"
                (click)="refreshEstimates(form)"
              >
                {{ 'Refresh estimates' | i18n }}
              </button>
            </div>
            <div class="capacity-list">
              @for (pkg of selectedPackages(form); track pkg.id) {
                <tui-accordion class="capacity-service">
                  <button
                    class="capacity-summary"
                    [tuiAccordion]="capacityDetailsOpen().has(pkg.id)"
                    (tuiAccordionChange)="
                      setCapacityDetailsOpen(pkg.id, !!$event)
                    "
                  >
                    <span tuiTitle>
                      <b>{{ pkg.name }}</b>
                      <span tuiSubtitle>
                        {{ 'Maximum required space' | i18n }}:
                        @if (capacityEstimate(pkg.id); as estimate) {
                          {{
                            bytes(estimate.conservativePeakExcludingManualBytes)
                          }}
                        } @else {
                          {{ 'Unknown' | i18n }}
                        }
                      </span>
                    </span>
                    <span class="g-primary more-info">
                      {{ 'More info' | i18n }}
                    </span>
                  </button>
                  <tui-expand>
                    @if (capacityEstimate(pkg.id); as estimate) {
                      <dl class="capacity-details">
                        <div>
                          <dt>{{ 'Live data estimate' | i18n }}</dt>
                          <dd>{{ bytes(estimate.liveLogicalBytes) }}</dd>
                        </div>
                        <div>
                          <dt>{{ 'Checkpoints' | i18n }}</dt>
                          <dd>
                            {{ estimate.retainedSnapshotCount }} /
                            {{ estimate.maximumProjectedSnapshotCount }}
                          </dd>
                        </div>
                        <div>
                          <dt>{{ 'Automatic storage' | i18n }}</dt>
                          <dd>{{ bytes(estimate.scheduledRetainedBytes) }}</dd>
                        </div>
                        <div>
                          <dt>{{ 'Next-run staging' | i18n }}</dt>
                          <dd>{{ bytes(estimate.stagingHeadroomBytes) }}</dd>
                        </div>
                      </dl>
                    } @else {
                      <p class="capacity-unknown">{{ 'Unknown' | i18n }}</p>
                    }
                  </tui-expand>
                </tui-accordion>
              }
            </div>
          </fieldset>

          @if (projectedCount(form) > 1) {
            <div tuiNotification appearance="warning">
              {{
                'Every retained version is a full copy. Each run also makes a full target-side staging copy. This can substantially increase storage use, runtime, and I/O, especially on network storage and slow external devices.'
                  | i18n
              }}
              <label class="check-row">
                <input
                  tuiCheckbox
                  type="checkbox"
                  formControlName="capacityConfirmed"
                />
                {{ 'I understand the full-copy storage impact' | i18n }}
              </label>
            </div>
          }

          <label class="checkbox-row first-backup">
            <input
              tuiCheckbox
              type="checkbox"
              formControlName="firstBackupNow"
            />
            <span tuiTitle>
              <b>
                {{
                  (form.id ? 'Run now' : 'Create the first backup now') | i18n
                }}
              </b>
              @if (!form.id) {
                <span tuiSubtitle>
                  {{ 'Recommended so protection begins immediately.' | i18n }}
                </span>
              }
            </span>
          </label>

          @if (!form.id) {
            <tui-textfield>
              <label tuiLabel>{{ 'Password' | i18n }}</label>
              <input
                tuiInput
                [type]="passwordMasked ? 'password' : 'text'"
                autocomplete="new-password"
                formControlName="password"
              />
              <button
                tuiIconButton
                type="button"
                size="xs"
                appearance="icon"
                [iconStart]="passwordMasked ? '@tui.eye' : '@tui.eye-off'"
                [attr.aria-label]="
                  (passwordMasked ? 'Show password' : 'Hide password') | i18n
                "
                (click)="passwordMasked = !passwordMasked"
              >
                {{
                  (passwordMasked ? 'Show password' : 'Hide password') | i18n
                }}
              </button>
            </tui-textfield>
            <tui-error formControlName="password" />
          }

          <p class="muted">
            {{ 'Timezone' | i18n }}: {{ form.timezone }} ·
            {{ 'Maximum automatic checkpoints per service' | i18n }}:
            {{ projectedCount(form) }}
          </p>
          <footer class="editor-actions">
            @if (selectedJob(); as job) {
              <button
                tuiButton
                type="button"
                appearance="primary-destructive"
                (click)="deleteJob(job)"
              >
                {{ 'Delete schedule' | i18n }}
              </button>
            }
            <button tuiButton type="submit">
              {{ 'Save' | i18n }}
            </button>
          </footer>
        </form>
      }

      @if (jobs().length <= 1 && editor()) {
        <div class="jobs-toolbar">
          <button
            tuiButton
            type="button"
            size="s"
            appearance="primary"
            iconStart="@tui.plus"
            (click)="create()"
          >
            {{ 'Add schedule' | i18n }}
          </button>
        </div>
      }

      @if (reassigning(); as job) {
        <form
          tuiCardLarge
          tuiForm="m"
          appearance="floating"
          [formGroup]="reassignForm"
          (submit.prevent)="reassign(job)"
        >
          <header tuiHeader class="heading">
            <h3 tuiTitle>
              <b>
                {{ 'Change backup location' | i18n }} — {{ jobName(job.id) }}
              </b>
            </h3>
            <button
              tuiButton
              type="button"
              size="xs"
              appearance="primary"
              (click)="cancelReassign(job)"
            >
              {{ 'Cancel' | i18n }}
            </button>
          </header>
          <div class="grid">
            <tui-textfield
              tuiChevron
              [stringify]="stringifyTarget"
              [tuiTextfieldCleaner]="false"
            >
              <label tuiLabel>{{ 'New backup location' | i18n }}</label>
              <input tuiSelect formControlName="targetId" />
              <tui-data-list *tuiDropdown>
                @for (target of backupService.targets(); track target.id) {
                  <button tuiOption [value]="target.id">
                    {{ target.name }}
                  </button>
                }
              </tui-data-list>
            </tui-textfield>
            <tui-error formControlName="targetId" />
            <tui-textfield>
              <label tuiLabel>{{ 'Password' | i18n }}</label>
              <input
                tuiInput
                [type]="reassignPasswordMasked ? 'password' : 'text'"
                autocomplete="new-password"
                formControlName="password"
              />
              <button
                tuiIconButton
                type="button"
                size="xs"
                appearance="icon"
                [iconStart]="
                  reassignPasswordMasked ? '@tui.eye' : '@tui.eye-off'
                "
                [attr.aria-label]="
                  (reassignPasswordMasked ? 'Show password' : 'Hide password')
                    | i18n
                "
                (click)="reassignPasswordMasked = !reassignPasswordMasked"
              >
                {{
                  (reassignPasswordMasked ? 'Show password' : 'Hide password')
                    | i18n
                }}
              </button>
            </tui-textfield>
            <tui-error formControlName="password" />
            <label class="switch-row">
              <input
                tuiSwitch
                type="checkbox"
                formControlName="waitForSchedule"
              />
              <span>{{ 'Wait for next automatic run' | i18n }}</span>
            </label>
          </div>
          <div tuiNotification appearance="warning">
            {{
              'Existing checkpoints are not moved. They remain archived at the original backup location.'
                | i18n
            }}
          </div>
          <footer class="g-buttons">
            <button tuiButton>
              {{ 'Change backup location' | i18n }}
            </button>
          </footer>
        </form>
      }
    }
  `,
  styles: `
    :host {
      display: grid;
      gap: 1rem;
      margin-block-end: 2rem;
      min-inline-size: 0;
      container-type: inline-size;
    }

    [tuiTitle] {
      min-inline-size: 0;
      overflow-wrap: anywhere;
    }

    .heading {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 1rem;
      margin: 1.5rem 0 0.75rem;
    }

    .heading h3 {
      margin: 0;
    }

    .table-wrap {
      inline-size: 100%;
      max-inline-size: 100%;
      min-inline-size: 0;
      overflow-x: auto;
    }

    .actions {
      display: flex;
      align-items: center;
      flex-wrap: wrap;
      gap: 0.35rem;
    }

    .services-options {
      display: grid;
      gap: 1rem;
    }

    .jobs-toolbar {
      display: flex;
      align-items: center;
      justify-content: flex-end;
      gap: 1rem;
      min-inline-size: 0;
    }

    .editor-actions {
      display: flex;
      align-items: center;
      justify-content: space-between;
      flex-wrap: wrap;
      gap: 0.75rem;
    }

    .editor-actions > :last-child {
      margin-inline-start: auto;
    }

    .selected-job,
    .editor-heading,
    .setting-row,
    .checkbox-row,
    .inline-switch {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 1rem;
      inline-size: 100%;
      min-inline-size: 0;
      box-sizing: border-box;
    }

    .selected-job {
      flex-wrap: wrap;
      padding: 1rem;
      border: 1px solid var(--tui-border-normal);
      border-radius: var(--tui-radius-m);
    }

    .selected-job > [tuiTitle] {
      flex: 1 1 16rem;
    }

    .review {
      display: grid;
      gap: 1rem;
      margin-block-start: 1rem;
      padding: 1rem;
      min-inline-size: 0;
      overflow: hidden;
    }

    .review-actions {
      display: flex;
      flex-wrap: wrap;
      justify-content: flex-end;
      gap: 0.75rem;
    }

    .review .checkbox-row {
      inline-size: 100%;
      max-inline-size: 100%;
      padding-inline: 0;
      align-items: center;
      justify-content: space-between;
    }

    .review .checkbox-row > input {
      inset-block-start: 0;
      margin-inline-start: auto;
      transform: none;
    }

    .review .toggle-all {
      justify-content: flex-end;
    }

    .review .toggle-all > input {
      margin-inline-start: 0;
    }

    .review-job [tuiTitle] {
      text-align: start;
    }

    .setting-row.vertical {
      align-items: stretch;
      flex-direction: column;
    }

    [tuiGroup] {
      inline-size: 100%;
    }

    [tuiBlock] img {
      inline-size: 2.5rem;
      border-radius: 50%;
    }

    [tuiBlock] [tuiTitle],
    .include-future [tuiTitle] {
      flex: 1;
    }

    [tuiBlock],
    [tuiBlock] [tuiTitle] {
      justify-content: flex-start;
      text-align: start;
    }

    .toggle-all,
    .first-backup {
      inline-size: fit-content;
      max-inline-size: 100%;
      justify-content: flex-start;
    }

    .toggle-all {
      inline-size: 100%;
      gap: 0.5rem;
      padding: 0 1rem 1rem;
      border-block-end: 1px solid var(--tui-border-normal);
      box-sizing: border-box;
    }

    .include-future {
      align-items: flex-start;
      inline-size: 100%;
      max-inline-size: 100%;
      padding-block: 0.75rem;
      padding-inline: 1rem;
      border-radius: var(--tui-radius-m);
      background: var(--tui-background-accent-2);
      color: var(--tui-text-primary-on-accent-2);
      box-sizing: border-box;
    }

    .include-future [tuiSubtitle] {
      color: inherit;
    }

    .inline-switch.left {
      inline-size: fit-content;
      justify-content: flex-start;
    }

    .retention-heading .inline-switch {
      flex: 0 0 auto;
      justify-content: flex-start;
      inline-size: fit-content;
      margin-inline-start: auto;
    }

    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
      gap: 1rem;
      align-items: end;
    }

    label > span:first-child {
      display: block;
      margin-block-end: 0.35rem;
      color: var(--tui-text-secondary);
    }

    fieldset {
      display: grid;
      gap: 0.65rem;
      min-inline-size: 0;
      border: 1px solid var(--tui-border-normal);
      border-radius: 0.5rem;
    }

    .check-row,
    .switch-row {
      display: flex;
      align-items: center;
      gap: 0.65rem;
    }

    .switch-row {
      justify-content: flex-start;
      min-block-size: 2.75rem;
    }

    .muted,
    .snapshot-row {
      color: var(--tui-text-secondary);
    }

    .snapshot-row td {
      display: flex;
      flex-wrap: wrap;
      gap: 0.35rem;
    }

    .snapshot-row button,
    .actions button {
      max-inline-size: 100%;
      block-size: auto;
      white-space: normal;
      overflow-wrap: anywhere;
    }

    .grid > *,
    .histories td {
      min-inline-size: 0;
      overflow-wrap: anywhere;
    }

    .history-heading {
      margin-block-start: 2rem;
    }

    .view-all-jobs {
      inline-size: 100%;
      min-inline-size: 0;
      text-align: start;
      box-sizing: border-box;
    }

    .view-all-jobs [tuiTitle] {
      flex: 1;
    }

    .estimate-heading {
      margin: 0 0 0.5rem;
    }

    .capacity-list,
    .capacity-details {
      display: grid;
      min-inline-size: 0;
    }

    .capacity-list {
      gap: 0.5rem;
    }

    .capacity-service {
      min-inline-size: 0;
      overflow: hidden;
      border: 1px solid var(--tui-border-normal);
      border-radius: var(--tui-radius-m);
    }

    .capacity-summary {
      inline-size: 100%;
      min-inline-size: 0;
      min-block-size: 3.5rem;
      block-size: auto;
      white-space: normal;
    }

    .capacity-summary [tuiTitle] {
      flex: 1;
      min-inline-size: 0;
      text-align: start;
    }

    .capacity-summary [tuiSubtitle] {
      display: block;
      margin-block-start: 0.2rem;
      white-space: normal;
    }

    .more-info {
      flex: 0 0 auto;
      margin-inline-start: auto;
      white-space: nowrap;
    }

    .capacity-details {
      margin: 0;
      padding: 0 1rem 1rem;
    }

    .capacity-details div {
      display: grid;
      grid-template-columns: minmax(10rem, 1fr) auto;
      gap: 1rem;
      padding-block: 0.65rem;
      border-block-start: 1px solid var(--tui-border-normal);
    }

    .capacity-details dt {
      color: var(--tui-text-secondary);
      font-weight: 600;
    }

    .capacity-details dd {
      margin: 0;
      text-align: end;
    }

    .capacity-unknown {
      margin: 0;
      padding: 0 1rem 1rem;
      color: var(--tui-text-secondary);
    }

    @container (max-inline-size: 30rem) {
      form[tuiCardLarge] {
        padding: 1rem;
      }

      [tuiBlock] img,
      [tuiBlock] > tui-icon {
        display: none;
      }

      .include-future {
        flex-direction: column;
        gap: 0.5rem;
        padding-inline: 0.75rem;
      }

      .include-future [tuiTitle] {
        inline-size: 100%;
        min-inline-size: 0;
      }

      .heading,
      .jobs-toolbar,
      .selected-job,
      .editor-heading {
        align-items: stretch;
        flex-direction: column;
      }

      .heading > button,
      .editor-heading > button {
        align-self: flex-start;
      }

      .selected-job {
        padding: 0.75rem;
      }

      .selected-job {
        justify-content: flex-start;
      }

      .selected-job > [tuiTitle] {
        flex: 0 1 auto;
      }

      .retention-heading {
        align-items: flex-start;
      }

      .retention-heading > [tuiTitle] {
        flex: 1;
        min-inline-size: 0;
      }

      .retention-heading .inline-switch {
        flex: 0 0 auto;
        inline-size: fit-content;
      }

      .retention-heading .retention-toggle-label {
        display: none;
      }

      .capacity-details div {
        grid-template-columns: 1fr;
        gap: 0.2rem;
      }

      .capacity-details dd {
        text-align: start;
      }

      .capacity-summary {
        flex-wrap: wrap;
      }

      .capacity-summary .more-info {
        flex-basis: 100%;
        text-align: end;
      }
    }
  `,
  host: {
    class: 'g-wrap-content',
    '(window:beforeunload)': 'confirmBrowserExit($event)',
  },
  imports: [
    BackupJobName,
    DatePipe,
    FormsModule,
    ReactiveFormsModule,
    TuiAccordion,
    TuiAppearance,
    TuiBadge,
    TuiBlock,
    TuiButton,
    TuiCardLarge,
    TuiCell,
    TuiCheckbox,
    TuiChevron,
    TuiDataList,
    TuiDropdown,
    TuiError,
    TuiForm,
    TuiGroup,
    TuiHeader,
    TuiIcon,
    TuiInput,
    TuiLabel,
    TuiLoader,
    TuiNotification,
    TuiSelect,
    TuiSwitch,
    TuiTitle,
    BackupScheduleBrowser,
    BackupScheduleControls,
    BackupRetentionRules,
    i18nPipe,
  ],
})
export class ScheduledBackups {
  private readonly formBuilder = inject(NonNullableFormBuilder)
  readonly mode = input.required<'manage' | 'restore'>()
  readonly createRequest = input(false)
  readonly reviewPackageId = input('')
  readonly createRequestHandled = output<void>()
  readonly collapseRequested = output<string | null>()

  private readonly api = inject(ApiService)
  protected readonly backupService = inject(BackupService)
  private readonly dialogs = inject(DialogService)
  private readonly deleteSchedule = inject(DeleteScheduleService)
  private readonly errors = inject(ErrorService)
  private readonly i18n = inject(i18nPipe)
  private readonly injector = inject(Injector)
  private readonly isMobile = inject(WA_IS_MOBILE)
  private readonly destroyRef = inject(DestroyRef)
  private readonly jobNameInput =
    viewChild<ElementRef<HTMLInputElement>>('jobNameInput')
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly packageData = toSignal(this.patch.watch$('packageData'))
  private readonly tasks = inject(TaskService)

  private readonly liveJobs = toSignal(
    this.patch.watch$('scheduledBackups', 'jobs'),
  )
  protected readonly jobs = computed(() =>
    Object.values(this.liveJobs() || {}).sort((a, b) =>
      a.createdAt.localeCompare(b.createdAt),
    ),
  )
  protected readonly histories = signal<T.ServiceTargetHistory[]>([])
  protected readonly reviews = signal<T.NewServiceBackupReview[]>([])
  protected readonly visibleReviews = computed(() =>
    this.reviews().filter(
      review => review.packageId === this.reviewPackageId(),
    ),
  )
  protected readonly loading = signal(true)
  protected readonly editor = signal<JobEditor | null>(null)
  private showSingleJobList = false
  protected readonly selectedJobId = signal('')
  protected readonly showServices = signal(false)
  protected readonly reassigning = signal<T.BackupJob | null>(null)
  protected readonly estimates = signal<T.BackupServiceCapacityEstimate[]>([])
  private estimateRequest = 0
  protected readonly capacityDetailsOpen = signal<ReadonlySet<string>>(
    new Set(),
  )

  protected readonly reassignForm = this.formBuilder.group({
    targetId: ['', Validators.required],
    password: ['', Validators.required],
    waitForSchedule: [false],
  })
  protected passwordMasked = true
  protected reassignPasswordMasked = true
  private editorBaseline: string | null = null
  private pendingReview: T.NewServiceBackupReview | null = null

  constructor() {
    void this.initialize()
    effect(() => {
      if (!this.createRequest() || this.loading()) return
      this.createRequestHandled.emit()
      void this.create(this.visibleReviews()[0])
    })
  }

  private readonly reviewDecisions = new Map<string, Record<string, boolean>>()
  protected readonly systemPackageId = SYSTEM_PACKAGE_ID

  protected readonly stringifyTarget = (targetId: string) =>
    this.backupService.targetName(targetId)

  protected readonly packages = computed(() => [
    {
      id: SYSTEM_PACKAGE_ID,
      name: this.i18n.transform('System'),
      icon: '',
    },
    ...Object.entries(this.packageData() || {}).flatMap(([id, entry]) => {
      const manifest = getManifest(entry)
      return manifest ? [{ id, name: manifest.title, icon: entry.icon }] : []
    }),
  ])
  protected readonly packageIds = computed(() =>
    this.packages().map(pkg => pkg.id),
  )
  protected readonly selectedJob = computed(() =>
    this.jobs().find(job => job.id === this.selectedJobId()),
  )

  isEditorOpen(): boolean {
    return this.editor() !== null
  }

  private async initialize() {
    await this.backupService.getBackupTargets()
    await this.reload()
  }

  async reload() {
    this.loading.set(true)
    try {
      const [histories, reviews] = await Promise.all([
        this.api.getScheduledBackupHistories({}),
        this.api.getNewServiceBackupReviews({}),
      ])
      this.histories.set(histories)
      this.reviews.set(reviews)
      for (const review of reviews) {
        this.reviewDecisions.set(
          review.packageId,
          Object.fromEntries(
            this.jobs().map(job => [
              job.id,
              this.jobIncludesService(job, review.packageId),
            ]),
          ),
        )
      }
      const selected = this.jobs().find(job => job.id === this.selectedJobId())
      if (this.editor()?.id && !selected) this.editor.set(null)
      if (!this.editor()) {
        if (selected) {
          void this.edit(selected)
        } else if (this.jobs().length === 1 && !this.showSingleJobList) {
          void this.edit(this.jobs()[0])
        }
      }
    } catch (error) {
      this.errors.handleError(getErrorMessage(error))
    } finally {
      this.loading.set(false)
    }
  }

  protected async create(review?: T.NewServiceBackupReview) {
    if (!(await this.confirmDiscardChanges())) return
    const now = new Date()
    const form = new JobEditor(
      this.formBuilder,
      {
        name: '',
        targetId: this.backupService.targets()[0]?.id || '',
        packageIds: review
          ? [SYSTEM_PACKAGE_ID, review.packageId]
          : this.packages().map(pkg => pkg.id),
        includeFuture: !review,
        preservedSelectedPackageIds: [],
        preservedExcludedPackageIds: [],
        frequency: 'daily',
        minute: now.getMinutes(),
        hour: now.getHours(),
        weekday: now.getDay(),
        dayOfMonth: now.getDate(),
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC',
        keepAdditional: false,
        ...parseBackupRetentionTier(),
        additionalTiers: [],
        retentionOverrides: {},
        password: '',
        firstBackupNow: true,
        capacityConfirmed: false,
      },
      this.jobs(),
    )
    this.showServices.set(false)
    this.showSingleJobList = false
    this.reassigning.set(null)
    this.pendingReview = review || null
    this.selectedJobId.set('')
    this.editor.set(form)
    this.editorBaseline = this.editorSnapshot(form)
    await this.refreshEstimates(form)
    this.showNameInput(form)
  }

  protected async viewAllJobs() {
    if (!(await this.confirmDiscardChanges())) return
    this.reassigning.set(null)
    this.showSingleJobList = true
    this.selectedJobId.set('')
    this.editor.set(null)
    this.editorBaseline = null
    this.pendingReview = null
    this.showServices.set(false)
  }

  protected async cancelEditor() {
    if (!(await this.confirmDiscardChanges())) return
    this.selectedJobId.set('')
    this.editor.set(null)
    this.editorBaseline = null
    this.pendingReview = null
    this.showServices.set(false)
    this.collapseRequested.emit(null)
  }

  protected confirmBrowserExit(event: BeforeUnloadEvent) {
    if (!this.hasUnsavedChanges()) return
    event.preventDefault()
    event.returnValue = ''
  }

  private hasUnsavedChanges(): boolean {
    const form = this.editor()
    return !!(
      form &&
      this.editorBaseline !== null &&
      this.editorSnapshot(form) !== this.editorBaseline
    )
  }

  async confirmDiscardChanges(): Promise<boolean> {
    if (!this.hasUnsavedChanges()) return true
    const confirmed = await firstValueFrom(
      this.dialogs.openConfirm({
        label: 'Unsaved changes',
        size: 's',
        data: {
          content: 'Changes were not saved',
          yes: 'Discard changes',
          no: 'Back',
        },
      }),
      { defaultValue: false },
    )
    if (confirmed) {
      const form = this.editor()
      this.editorBaseline = form ? this.editorSnapshot(form) : null
    }
    return confirmed
  }

  protected isDefaultJob(form: JobEditor): boolean {
    return !!form.id && this.jobs().length === 1 && form.name === 'Default'
  }

  protected async edit(job?: T.BackupJob) {
    if (!job) return
    if (!(await this.confirmDiscardChanges())) return
    this.showSingleJobList = false
    this.showServices.set(false)
    this.pendingReview = null
    const schedule = parseBackupSchedule(job.schedule)
    const selection = parseBackupServiceSelection(
      job.services,
      this.packages().map(pkg => pkg.id),
    )
    const [tier, ...additionalTiers] = job.defaultRetention.tiers
    const retention = parseBackupRetentionTier(tier)
    const form = new JobEditor(
      this.formBuilder,
      {
        id: job.id,
        name: job.name,
        targetId: job.targetId,
        ...selection,
        ...schedule,
        keepAdditional: !!tier,
        ...retention,
        additionalTiers: additionalTiers.map(item =>
          parseBackupRetentionTier(item),
        ),
        retentionOverrides: Object.fromEntries(
          Object.entries(job.retentionOverrides).map(([packageId, policy]) => [
            packageId,
            {
              tiers: this.toTierEditors(policy),
            },
          ]),
        ),
        password: '',
        firstBackupNow: false,
        capacityConfirmed: false,
      },
      this.jobs(),
    )
    this.selectedJobId.set(job.id)
    this.editor.set(form)
    this.editorBaseline = this.editorSnapshot(form)
    void this.refreshEstimates(form)
  }

  protected removeRetentionRule(form: JobEditor, index: number) {
    const result = removeBackupRetentionRule<BackupRetentionTierEditor>(
      form,
      form.additionalTiers,
      index,
      parseBackupRetentionTier(),
    )
    Object.assign(form, result.primary)
    form.additionalTiers = result.additional
    form.keepAdditional = result.keepAdditional
    form.capacityConfirmed = false
  }

  protected retentionRules(form: JobEditor): BackupRetentionTierEditor[] {
    return [form, ...form.additionalTiers]
  }

  protected updateRetentionRule(
    form: JobEditor,
    index: number,
    value: BackupRetentionRuleValue,
  ) {
    const rule = this.retentionRules(form)[index]
    if (!rule) return
    Object.assign(rule, value)
    form.capacityConfirmed = false
  }

  protected addRetentionRule(form: JobEditor) {
    form.additionalTiers.push(parseBackupRetentionTier())
    form.capacityConfirmed = false
  }

  protected togglePackage(
    form: JobEditor,
    packageId: string,
    checked: boolean,
  ) {
    form.packageIds = checked
      ? [...new Set([...form.packageIds, packageId])]
      : form.packageIds.filter(id => id !== packageId)
    if (!checked) delete form.retentionOverrides[packageId]
    form.capacityConfirmed = false
  }

  protected allPackagesSelected(form: JobEditor): boolean {
    const services = this.packages().filter(pkg => pkg.id !== SYSTEM_PACKAGE_ID)
    return (
      services.length > 0 &&
      services.every(pkg => form.packageIds.includes(pkg.id))
    )
  }

  protected setAllPackages(form: JobEditor, checked: boolean) {
    const includesSystem = form.packageIds.includes(SYSTEM_PACKAGE_ID)
    const services = this.packages().filter(pkg => pkg.id !== SYSTEM_PACKAGE_ID)
    form.packageIds = [
      ...(includesSystem ? [SYSTEM_PACKAGE_ID] : []),
      ...(checked ? services.map(service => service.id) : []),
    ]
    if (!checked) {
      services.forEach(service => delete form.retentionOverrides[service.id])
    }
    form.capacityConfirmed = false
  }

  protected async save(form: JobEditor) {
    form.form.markAllAsTouched()
    if (form.form.controls.name.invalid) {
      this.showNameInput(form)
      return
    }
    if (!this.canSave(form)) return
    const common = {
      name: form.form.getRawValue().name,
      services: serializeBackupServiceSelection(
        form,
        this.packages().map(pkg => pkg.id),
      ),
      schedule: serializeBackupSchedule(form),
      defaultRetention: this.defaultPolicy(form),
      retentionOverrides: Object.fromEntries(
        Object.entries(form.retentionOverrides).map(([packageId, override]) => [
          packageId,
          serializeBackupRetentionPolicy(override.tiers),
        ]),
      ),
    }
    const valid = await this.tasks.run(
      () =>
        this.api.validateScheduledBackupJob({
          id: form.id || null,
          services: common.services,
          schedule: common.schedule,
          defaultRetention: common.defaultRetention,
          retentionOverrides: common.retentionOverrides,
        }),
      'Validating',
    )
    if (!valid) return
    const retentionChanges = form.id
      ? await this.confirmJobRetentionChanges(form)
      : []
    if (retentionChanges === null) return
    await this.tasks.run(async () => {
      if (form.id) {
        const selectedJob = this.selectedJob()
        await this.api.updateScheduledBackupJob({
          id: form.id!,
          ...common,
        })
        for (const change of retentionChanges) {
          await this.api.updateScheduledRetention({
            targetId: change.history.targetId,
            packageId: change.history.packageId,
            policy: change.policy,
            confirmedRemovals: change.preview.removed.map(
              snapshot => snapshot.id,
            ),
          })
        }
        this.finishSave(form.firstBackupNow ? form.id! : null)
        if (form.firstBackupNow && selectedJob && !selectedJob.enabled) {
          await this.api.setScheduledBackupJobEnabled({
            id: form.id!,
            enabled: true,
          })
        }
        if (form.firstBackupNow) {
          await this.waitForRunProgress(
            form.id!,
            this.api.runScheduledBackupJob({ id: form.id! }),
          )
        }
        return
      } else {
        const created = await this.backupService.withOriginalPassword(
          oldPassword =>
            this.api.createScheduledBackupJob({
              ...common,
              targetId: form.targetId,
              password: form.password,
              oldPassword,
              enabled: true,
              runNow: form.firstBackupNow,
            }),
        )
        if (!created) return
        this.selectedJobId.set(created.id)
        this.backupService.showQueuedNotification(created)
        if (
          this.pendingReview &&
          form.packageIds.includes(this.pendingReview.packageId)
        ) {
          await this.api.resolveNewServiceBackupReview({
            packageId: this.pendingReview.packageId,
            decisions: Object.fromEntries([
              ...this.jobs().map(job => [job.id, false] as const),
              [created.id, true] as const,
            ]),
          })
        }
      }
      this.finishSave(null)
      await this.reload()
    }, 'Saving')
  }

  private jobRetentionChanges(form: JobEditor) {
    const job = this.jobs().find(candidate => candidate.id === form.id)
    if (!job) return []
    const selected = new Set(this.selectedPackages(form).map(pkg => pkg.id))
    const defaultPolicy = this.defaultPolicy(form)
    const overrides = Object.fromEntries(
      Object.entries(form.retentionOverrides).map(([packageId, override]) => [
        packageId,
        serializeBackupRetentionPolicy(override.tiers),
      ]),
    )
    return this.histories().flatMap(history => {
      if (
        !selected.has(history.packageId) ||
        history.feedingJobs.length !== 1 ||
        history.feedingJobs[0] !== job.id
      ) {
        return []
      }
      const policy = overrides[history.packageId] || defaultPolicy
      const previousPolicy =
        job.retentionOverrides[history.packageId] || job.defaultRetention
      if (JSON.stringify(policy) === JSON.stringify(previousPolicy)) return []
      return JSON.stringify(policy) === JSON.stringify(history.policy)
        ? []
        : [{ history, policy }]
    })
  }

  private async confirmJobRetentionChanges(
    form: JobEditor,
  ): Promise<ConfirmedRetentionChange[] | null> {
    const candidates = this.jobRetentionChanges(form)
    let changes: ConfirmedRetentionChange[] = []
    const loaded = await this.tasks.run(async () => {
      changes = await Promise.all(
        candidates.map(async ({ history, policy }) => ({
          history,
          policy,
          preview: await this.api.previewScheduledRetention({
            targetId: history.targetId,
            packageId: history.packageId,
            policy,
          }),
        })),
      )
    }, 'Loading')
    if (!loaded) return null
    if (!changes.some(change => change.preview.removed.length)) return changes
    const confirmed = await firstValueFrom(
      this.dialogs.openComponent<boolean>(BACKUP_RETENTION_CONFIRM, {
        size: 'm',
        data: changes
          .filter(change => change.preview.removed.length)
          .map(change => ({
            packageId: change.history.packageId,
            name: this.packageName(change.history.packageId),
            removed: change.preview.removed,
          })),
      }),
      { defaultValue: false },
    )
    return confirmed ? changes : null
  }

  private showNameInput(form: JobEditor) {
    if (this.editor() !== form || this.destroyRef.destroyed) return
    afterNextRender(
      () => {
        const input = this.jobNameInput()?.nativeElement
        if (this.isMobile) {
          input?.scrollIntoView({ behavior: 'smooth', block: 'center' })
        } else {
          input?.focus()
        }
      },
      { injector: this.injector },
    )
  }

  private finishSave(runNowJobId: string | null) {
    this.selectedJobId.set('')
    this.editor.set(null)
    this.editorBaseline = null
    this.pendingReview = null
    this.showSingleJobList = true
    this.collapseRequested.emit(runNowJobId)
  }

  private async waitForRunProgress(
    jobId: string,
    run: Promise<T.BackupRun>,
  ): Promise<void> {
    const outcome = await firstValueFrom(
      race(
        from(run).pipe(map(() => 'completed' as const)),
        this.patch.watch$('scheduledBackups', 'activities').pipe(
          filter(activities =>
            Object.values(activities).some(
              activity =>
                activity.jobId === jobId && activity.state === 'running',
            ),
          ),
          map(() => 'visible' as const),
        ),
      ).pipe(takeUntilDestroyed(this.destroyRef)),
      { defaultValue: 'destroyed' as const },
    )
    if (outcome !== 'completed') {
      void run.catch(error => this.errors.handleError(getErrorMessage(error)))
    }
  }

  protected async runNow(job: T.BackupJob) {
    await this.performAndReload(() =>
      this.api.runScheduledBackupJob({ id: job.id }),
    )
  }

  protected async setJobEnabled(job: T.BackupJob, enabled: boolean) {
    if (enabled === job.enabled && !job.pause) return
    await this.performAndReload(() =>
      this.api.setScheduledBackupJobEnabled({
        id: job.id,
        enabled,
      }),
    )
  }

  protected async deleteJob(job: T.BackupJob) {
    if (await this.deleteSchedule.delete(job)) {
      this.showSingleJobList = true
      this.selectedJobId.set('')
      this.editor.set(null)
      this.editorBaseline = null
      this.pendingReview = null
      await this.reload()
      if (this.jobs().length <= 1) this.collapseRequested.emit(null)
    }
  }

  protected async retry(job: T.BackupJob) {
    const password = await firstValueFrom(
      this.dialogs.openPrompt<string>({
        label: 'Enter password',
        data: {
          message: 'Enter password',
          label: 'Password',
          placeholder: 'Password',
          buttonText: 'Retry',
          useMask: true,
        },
      }),
      { defaultValue: '' },
    )
    if (!password) return
    await this.performAndReload(() =>
      this.backupService.withOriginalPassword(oldPassword =>
        this.api.retryScheduledBackupTarget({
          targetId: job.targetId,
          password,
          oldPassword,
        }),
      ),
    )
  }

  protected async beginReassign(job: T.BackupJob) {
    if (!(await this.confirmDiscardChanges())) return
    this.editor.set(null)
    this.editorBaseline = null
    this.pendingReview = null
    this.showServices.set(false)
    this.reassigning.set(job)
    this.reassignForm.reset({
      targetId:
        this.backupService.targets().find(t => t.id !== job.targetId)?.id || '',
      password: '',
      waitForSchedule: false,
    })
  }

  protected cancelReassign(job: T.BackupJob) {
    this.reassigning.set(null)
    void this.edit(job)
  }

  protected async reassign(job: T.BackupJob) {
    this.reassignForm.markAllAsTouched()
    if (this.reassignForm.invalid) return
    const reassign = this.reassignForm.getRawValue()
    await this.tasks.run(async () => {
      const reassigned = await this.backupService.withOriginalPassword(
        oldPassword =>
          this.api.reassignScheduledBackupTarget({
            id: job.id,
            targetId: reassign.targetId,
            password: reassign.password,
            oldPassword,
            waitForSchedule: reassign.waitForSchedule,
          }),
      )
      if (!reassigned) return
      this.reassigning.set(null)
      await this.reload()
    }, 'Saving')
  }

  protected reviewDecision(packageId: string, jobId: string): boolean {
    return this.reviewDecisions.get(packageId)?.[jobId] ?? false
  }

  protected setReviewDecision(
    packageId: string,
    jobId: string,
    value: boolean,
  ) {
    const decisions = this.reviewDecisions.get(packageId) || {}
    decisions[jobId] = value
    this.reviewDecisions.set(packageId, decisions)
  }

  protected allReviewJobsSelected(review: T.NewServiceBackupReview): boolean {
    const decisions = this.reviewDecisions.get(review.packageId)
    return (
      this.jobs().length > 0 &&
      this.jobs().every(job => decisions?.[job.id] === true)
    )
  }

  protected setAllReviewJobs(
    review: T.NewServiceBackupReview,
    checked: boolean,
  ) {
    this.reviewDecisions.set(
      review.packageId,
      Object.fromEntries(this.jobs().map(job => [job.id, checked])),
    )
  }

  protected async resolveReview(review: T.NewServiceBackupReview) {
    const selected = this.reviewDecisions.get(review.packageId) || {}
    const decisions = Object.fromEntries(
      this.jobs().map(job => [job.id, selected[job.id] === true]),
    )
    await this.performAndReload(() =>
      this.api.resolveNewServiceBackupReview({
        packageId: review.packageId,
        decisions,
      }),
    )
  }

  private jobIncludesService(job: T.BackupJob, packageId: string): boolean {
    return (
      job.services.type === 'all' ||
      (job.services.type === 'allExcept' &&
        !job.services.excludedPackageIds.includes(packageId)) ||
      (job.services.type === 'selected' &&
        job.services.packageIds.includes(packageId))
    )
  }

  protected jobName(id: string): string {
    return this.jobs().find(job => job.id === id)?.name || id
  }

  protected packageName(id: string): string {
    if (id === SYSTEM_PACKAGE_ID) return this.i18n.transform('System')
    return this.packages().find(pkg => pkg.id === id)?.name || id
  }

  protected readonly pauseLabel = backupPauseLabel
  protected scheduleSummary(form: JobEditor): string {
    return formatBackupScheduleSummary(form, label =>
      this.i18n.transform(label),
    )
  }

  protected updateSchedule(form: JobEditor, schedule: BackupScheduleFormValue) {
    Object.assign(form, schedule)
  }

  protected selectedServiceSummary(form: JobEditor): string {
    const total = this.packages().filter(
      pkg => pkg.id !== SYSTEM_PACKAGE_ID,
    ).length
    const selected = form.packageIds.filter(
      id => id !== SYSTEM_PACKAGE_ID,
    ).length
    return formatBackupServiceSummary(
      selected,
      total,
      form.includeFuture,
      form.packageIds.includes(SYSTEM_PACKAGE_ID),
      label => this.i18n.transform(label),
    )
  }

  protected retentionSummary(form: JobEditor): string {
    if (!form.keepAdditional) {
      return this.i18n.transform('Keep only the latest automatic checkpoint')
    }
    return [form, ...form.additionalTiers]
      .map(rule =>
        formatBackupRetentionRule(rule, label => this.i18n.transform(label)),
      )
      .join(' · ')
  }

  protected canSave(form: JobEditor): boolean {
    return !!(
      form.name.trim() &&
      form.targetId &&
      form.packageIds.length &&
      isValidBackupSchedule(form) &&
      this.validRetention(form) &&
      (form.id || form.password) &&
      (this.projectedCount(form) <= 1 || form.capacityConfirmed)
    )
  }

  private validRetention(form: JobEditor): boolean {
    if (!form.keepAdditional) return true
    return isValidBackupRetentionRules([form, ...form.additionalTiers])
  }

  protected retentionHasDuplicates(form: JobEditor): boolean {
    return hasDuplicateRetentionRules([form, ...form.additionalTiers])
  }

  protected projectedCount(form: JobEditor): number {
    return Math.max(
      this.maximumProjected(this.defaultPolicy(form)),
      ...Object.values(form.retentionOverrides).map(override =>
        this.maximumProjected(serializeBackupRetentionPolicy(override.tiers)),
      ),
    )
  }

  protected selectedPackages(form: JobEditor) {
    return this.packages().filter(pkg => form.packageIds.includes(pkg.id))
  }

  protected capacityEstimate(packageId: string) {
    return this.estimates().find(estimate => estimate.packageId === packageId)
  }

  protected setCapacityDetailsOpen(packageId: string, open: boolean) {
    this.capacityDetailsOpen.update(current => {
      const next = new Set(current)
      if (open) next.add(packageId)
      else next.delete(packageId)
      return next
    })
  }

  protected async refreshEstimates(form: JobEditor) {
    const request = ++this.estimateRequest
    const snapshot = this.editorSnapshot(form)
    this.estimates.set([])
    if (!form.targetId) return
    await this.tasks.run(async () => {
      const estimates = await this.api.estimateScheduledBackupCapacity({
        targetId: form.targetId,
        services: serializeBackupServiceSelection(
          form,
          this.packages().map(pkg => pkg.id),
        ),
        defaultRetention: this.defaultPolicy(form),
        preserveExistingPolicies: !form.id,
        retentionOverrides: Object.fromEntries([
          ...Object.entries(form.retentionOverrides).map(
            ([packageId, override]) => [
              packageId,
              serializeBackupRetentionPolicy(override.tiers),
            ],
          ),
          ...(form.id
            ? this.histories()
                .filter(
                  history =>
                    history.targetId === form.targetId &&
                    (history.snapshots.length || history.feedingJobs.length),
                )
                .map(history => [history.packageId, history.policy])
            : []),
          ...this.jobRetentionChanges(form).map(({ history, policy }) => [
            history.packageId,
            policy,
          ]),
        ]),
      })
      if (
        request === this.estimateRequest &&
        this.editor() === form &&
        this.editorSnapshot(form) === snapshot
      ) {
        this.estimates.set(estimates)
      }
    }, 'Loading')
  }

  protected maximumProjected(policy: T.RetentionPolicy): number {
    return (
      1 +
      policy.tiers.reduce(
        (sum, tier) =>
          sum + Math.ceil(tier.coverageSeconds / tier.intervalSeconds),
        0,
      )
    )
  }

  protected bytes(value: number | null): string {
    return value === null ? '—' : convertBytes(value)
  }

  private defaultPolicy(form: JobEditor): T.RetentionPolicy {
    return serializeBackupRetentionPolicy(
      form.keepAdditional ? [form, ...form.additionalTiers] : [],
    )
  }

  private toTierEditors(
    policy: T.RetentionPolicy,
  ): BackupRetentionTierEditor[] {
    return policy.tiers.map(tier => parseBackupRetentionTier(tier))
  }

  private editorSnapshot(form: JobEditor): string {
    return JSON.stringify(form)
  }

  private async performAndReload<T>(action: () => Promise<T>) {
    await this.tasks.run(async () => {
      await action()
      await this.reload()
    }, 'Saving')
  }
}
