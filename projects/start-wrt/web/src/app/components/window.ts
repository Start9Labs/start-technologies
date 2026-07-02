import { Component, inject } from '@angular/core'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidationErrors,
} from '@angular/forms'
import { TuiTime } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiGroup,
  TuiLabel,
} from '@taiga-ui/core'
import { TuiBlock, TuiDataListWrapper, TuiInputTime } from '@taiga-ui/kit'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { ModalHelp } from 'src/app/help/modal-help'
import { ScheduleWindow } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { coversFullWeek, windowsOverlap } from 'src/app/utils/schedule'

/**
 * Dialog data for {@link AddWindow}. `edit` discriminates edit-vs-add mode
 * explicitly so the dialog never parses the (translatable) label to decide.
 * `others` are the sibling windows used for wrap-aware overlap validation.
 */
export type WindowData = ScheduleWindow & {
  edit: boolean
  others?: ScheduleWindow[]
}

@Component({
  template: `
    <form
      tuiForm="m"
      class="g-form"
      [formGroup]="form"
      (submit.prevent)="save()"
    >
      <fieldset>
        <legend>{{ 'Time Window' | i18n }}</legend>
        <tui-textfield>
          <label tuiLabel>{{ 'Start Time' | i18n }}</label>
          <input tuiInputTime mode="HH:MM AA" formControlName="startTime" />
          <tui-data-list-wrapper
            *tuiDropdown
            [items]="quarterHours"
            [itemContent]="timeItem"
          />
        </tui-textfield>
        <tui-textfield>
          <label tuiLabel>{{ 'End Time' | i18n }}</label>
          <input tuiInputTime mode="HH:MM AA" formControlName="endTime" />
          <tui-data-list-wrapper
            *tuiDropdown
            [items]="endQuarterHours"
            [itemContent]="timeItem"
          />
        </tui-textfield>
        <ng-template #timeItem let-time>
          {{ time.toString('HH:MM AA') }}
        </ng-template>
      </fieldset>
      <tui-error [formGroup]="form" />
      <fieldset formGroupName="days" [style.display]="'flex'">
        <legend>{{ 'Days' | i18n }}</legend>
        <div tuiGroup [collapsed]="true">
          @for (day of displayDays; track day.key) {
            <label tuiBlock="s" appearance="">
              <input type="checkbox" tuiBlock="s" [formControlName]="day.key" />
              {{ day.label | i18n }}
            </label>
          }
        </div>
      </fieldset>
      <footer>
        @if (context.data.edit) {
          <button
            tuiButton
            type="button"
            appearance="secondary-destructive"
            [style.margin-inline-end]="'auto'"
            (click)="remove()"
          >
            {{ 'Remove' | i18n }}
          </button>
        }
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  styles: `
    [tuiGroup] {
      width: 100%;
      height: var(--tui-height-m);
    }
  `,
  hostDirectives: [ModalHelp],
  providers: [
    provideTranslatedValidationErrors({
      overlap: 'This window overlaps another schedule',
      fullWeek:
        'Schedule covers the whole week — disable WiFi/WAN directly instead',
    }),
  ],
  imports: [
    ReactiveFormsModule,
    TuiForm,
    TuiLabel,
    TuiButton,
    TuiError,
    TuiInputTime,
    TuiDataListWrapper,
    TuiGroup,
    TuiBlock,
    i18nPipe,
  ],
})
export class AddWindow {
  protected readonly builder = inject(NonNullableFormBuilder)
  protected readonly context =
    injectContext<TuiDialogContext<ScheduleWindow | null, WindowData>>()

  // 15-minute increments across the day for the start-time picker dropdown.
  protected readonly quarterHours: readonly TuiTime[] = Array.from(
    { length: 96 },
    (_, i) => new TuiTime(Math.floor(i / 4), (i % 4) * 15),
  )

  // End-time dropdown additionally offers a trailing 12:00 AM (reusing the
  // 00:00 entry) so a window can close at end-of-day midnight. Stored as
  // "00:00"; the wrap-aware logic treats end <= start as crossing into the
  // next day, so picking it with a 12:00 AM start yields a full 24h blackout.
  protected readonly endQuarterHours: readonly TuiTime[] = [
    ...this.quarterHours,
    this.quarterHours[0],
  ]

  // Display Mon–Sun; data keys map to Sun(0)–Sat(6)
  protected readonly displayDays = [
    { label: 'Mon', key: 1 },
    { label: 'Tue', key: 2 },
    { label: 'Wed', key: 3 },
    { label: 'Thu', key: 4 },
    { label: 'Fri', key: 5 },
    { label: 'Sat', key: 6 },
    { label: 'Sun', key: 0 },
  ]

  protected readonly form = this.builder.group(
    {
      startTime: [TuiTime.fromString(this.context.data.startTime)],
      endTime: [TuiTime.fromString(this.context.data.endTime)],
      days: this.builder.group({
        0: [this.context.data.days[0]],
        1: [this.context.data.days[1]],
        2: [this.context.data.days[2]],
        3: [this.context.data.days[3]],
        4: [this.context.data.days[4]],
        5: [this.context.data.days[5]],
        6: [this.context.data.days[6]],
      }),
    },
    {
      validators: (control: AbstractControl): ValidationErrors | null => {
        const start = control.get('startTime')?.value as TuiTime | null
        const end = control.get('endTime')?.value as TuiTime | null
        if (!start || !end) return null

        // end < start wraps past midnight (e.g. 22:00-06:00); end == start is a
        // full 24-hour window (e.g. 09:00-09:00 next day). Both are valid.
        // Block submission if this window would overlap any other (wrap-aware).
        const daysVal = control.get('days')?.value as Record<number, boolean>
        const days = Array.from({ length: 7 }, (_, i) => !!daysVal?.[i]) as [
          boolean,
          boolean,
          boolean,
          boolean,
          boolean,
          boolean,
          boolean,
        ]
        const candidate: ScheduleWindow = {
          startTime: start.toString('HH:MM'),
          endTime: end.toString('HH:MM'),
          days,
        }
        const all = [candidate, ...(this.context.data.others ?? [])]
        if (windowsOverlap(all)) return { overlap: true }
        // A schedule with no gaps produces no cron edges; the backend rejects it.
        if (coversFullWeek(all)) return { fullWeek: true }
        return null
      },
    },
  )

  protected save(): void {
    if (this.form.invalid) return
    const val = this.form.getRawValue()
    const daysObj = val.days as Record<number, boolean>
    const days = Array.from({ length: 7 }, (_, i) => daysObj[i]) as [
      boolean,
      boolean,
      boolean,
      boolean,
      boolean,
      boolean,
      boolean,
    ]

    this.context.completeWith({
      startTime: val.startTime.toString('HH:MM'),
      endTime: val.endTime.toString('HH:MM'),
      days,
    })
  }

  protected remove(): void {
    this.context.completeWith(null)
  }
}
