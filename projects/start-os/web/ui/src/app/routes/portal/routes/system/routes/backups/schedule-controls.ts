import {
  Component,
  computed,
  effect,
  inject,
  input,
  output,
} from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import { TuiDataList } from '@taiga-ui/core'
import { TuiChevron, TuiSelect } from '@taiga-ui/kit'

import {
  BACKUP_FREQUENCIES,
  BACKUP_HOURS,
  BACKUP_MINUTES,
  BACKUP_MONTH_DAYS,
  BACKUP_WEEKDAYS,
  backupFrequencyLabel,
  BackupScheduleFormValue,
  BackupScheduleFrequency,
  backupTimezones,
  backupWeekdayLabel,
  formatBackupTime,
} from './scheduled-utils'

@Component({
  selector: 'backup-schedule-controls',
  template: `
    <div class="controls" [formGroup]="form">
      <tui-textfield
        tuiChevron
        [stringify]="stringifyFrequency"
        [tuiTextfieldCleaner]="false"
      >
        <label tuiLabel>{{ 'Frequency' | i18n }}</label>
        <input tuiSelect formControlName="frequency" />
        <tui-data-list *tuiDropdown>
          @for (frequency of frequencies; track frequency) {
            <button tuiOption [value]="frequency">
              {{ stringifyFrequency(frequency) }}
            </button>
          }
        </tui-data-list>
      </tui-textfield>

      <tui-textfield tuiChevron [tuiTextfieldCleaner]="false">
        <label tuiLabel>{{ 'Timezone' | i18n }}</label>
        <input tuiSelect formControlName="timezone" />
        <tui-data-list *tuiDropdown>
          @for (timezone of timezones(); track timezone) {
            <button tuiOption [value]="timezone">{{ timezone }}</button>
          }
        </tui-data-list>
      </tui-textfield>

      @if (formValue().frequency === 'weekly') {
        <tui-textfield
          tuiChevron
          [stringify]="stringifyWeekday"
          [tuiTextfieldCleaner]="false"
        >
          <label tuiLabel>{{ 'Day of week' | i18n }}</label>
          <input tuiSelect formControlName="weekday" />
          <tui-data-list *tuiDropdown>
            @for (day of weekdays; track day.value) {
              <button tuiOption [value]="day.value">
                {{ stringifyWeekday(day.value) }}
              </button>
            }
          </tui-data-list>
        </tui-textfield>
      }

      @if (formValue().frequency === 'monthly') {
        <tui-textfield tuiChevron [tuiTextfieldCleaner]="false">
          <label tuiLabel>{{ 'Day of month' | i18n }}</label>
          <input tuiSelect formControlName="dayOfMonth" />
          <tui-data-list *tuiDropdown>
            @for (day of monthDays; track day) {
              <button tuiOption [value]="day">{{ day }}</button>
            }
          </tui-data-list>
        </tui-textfield>
      }

      @if (formValue().frequency !== 'hourly') {
        <tui-textfield
          tuiChevron
          [stringify]="stringifyTime"
          [tuiTextfieldCleaner]="false"
        >
          <label tuiLabel>{{ 'Hour' | i18n }}</label>
          <input tuiSelect formControlName="hour" />
          <tui-data-list *tuiDropdown>
            @for (hour of hours; track hour) {
              <button tuiOption [value]="hour">
                {{ stringifyTime(hour) }}
              </button>
            }
          </tui-data-list>
        </tui-textfield>
      }

      <tui-textfield
        tuiChevron
        [stringify]="stringifyTime"
        [tuiTextfieldCleaner]="false"
      >
        <label tuiLabel>{{ 'Minute' | i18n }}</label>
        <input tuiSelect formControlName="minute" />
        <tui-data-list *tuiDropdown>
          @for (minute of minutes; track minute) {
            <button tuiOption [value]="minute">
              {{ stringifyTime(minute) }}
            </button>
          }
        </tui-data-list>
      </tui-textfield>
    </div>
  `,
  styles: `
    :host {
      display: block;
      inline-size: 100%;
      min-inline-size: 0;
    }

    .controls {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr));
      gap: 0.75rem;
      align-items: end;
      inline-size: 100%;
      min-inline-size: 0;
    }

    .controls > *,
    tui-textfield {
      inline-size: 100%;
      min-inline-size: 0;
    }

    @container (max-inline-size: 48rem) {
      .controls {
        grid-template-columns: 1fr 1fr;
      }
    }

    @container (max-inline-size: 30rem) {
      .controls {
        grid-template-columns: 1fr;
      }
    }
  `,
  imports: [ReactiveFormsModule, TuiChevron, TuiDataList, TuiSelect, i18nPipe],
})
export class BackupScheduleControls {
  private readonly i18n = inject(i18nPipe)

  readonly schedule = input.required<BackupScheduleFormValue>()
  readonly scheduleChange = output<BackupScheduleFormValue>()
  protected readonly form = inject(NonNullableFormBuilder).group({
    frequency: ['daily' as BackupScheduleFrequency, Validators.required],
    minute: [0, [Validators.required, Validators.min(0), Validators.max(59)]],
    hour: [0, [Validators.required, Validators.min(0), Validators.max(23)]],
    weekday: [0, [Validators.required, Validators.min(0), Validators.max(6)]],
    dayOfMonth: [
      1,
      [Validators.required, Validators.min(1), Validators.max(31)],
    ],
    timezone: ['', Validators.required],
  })
  protected readonly formValue = toSignal(this.form.valueChanges, {
    initialValue: this.form.getRawValue(),
  })

  protected readonly frequencies = BACKUP_FREQUENCIES
  protected readonly weekdays = BACKUP_WEEKDAYS
  protected readonly hours = BACKUP_HOURS
  protected readonly minutes = BACKUP_MINUTES
  protected readonly monthDays = BACKUP_MONTH_DAYS
  protected readonly stringifyTime = formatBackupTime
  protected readonly stringifyFrequency = (
    frequency: BackupScheduleFrequency,
  ) => this.i18n.transform(backupFrequencyLabel(frequency))
  protected readonly stringifyWeekday = (weekday: number) =>
    this.i18n.transform(backupWeekdayLabel(weekday))
  protected readonly timezones = computed(() =>
    backupTimezones(this.schedule().timezone),
  )
  private readonly scheduleChangesSubscription = this.form.valueChanges
    .pipe(takeUntilDestroyed())
    .subscribe(() => this.scheduleChange.emit(this.form.getRawValue()))

  constructor() {
    effect(() => this.form.reset(this.schedule()))
  }
}
