import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { IST } from '@start9labs/start-core'
import {
  TUI_FIRST_DAY,
  TUI_LAST_DAY,
  TuiDay,
  TuiMapperPipe,
  TuiTime,
} from '@taiga-ui/cdk'
import { TuiIcon, TuiInput } from '@taiga-ui/core'
import {
  TuiInputDate,
  TuiInputDateTime,
  TuiInputTime,
  TuiTooltip,
} from '@taiga-ui/kit'

import { Control } from './control'
import { HintPipe } from '../pipes/hint.pipe'

@Component({
  selector: 'form-datetime',
  template: `
    <tui-textfield
      [invalid]="control.invalid()"
      (tuiActiveZoneChange)="!$event && control.onTouched()"
    >
      @if (spec.name) {
        <label tuiLabel>
          {{ spec.name }}
          @if (spec.required) {
            <span>*</span>
          }
        </label>
      }
      @if (spec | hint; as hint) {
        <tui-icon [tuiTooltip]="hint" />
      }
      @if (spec.inputmode !== 'time') {
        <tui-calendar *tuiDropdown />
      }
      @switch (spec.inputmode) {
        @case ('time') {
          <input
            tuiInputTime
            type="time"
            [readonly]="readOnly"
            [disabled]="!!spec.disabled"
            [ngModel]="value | tuiMapper: getTime"
            (ngModelChange)="value = $event?.toString() || null"
            (blur)="control.onTouched()"
          />
        }
        @case ('date') {
          <input
            tuiInputDate
            type="date"
            [readonly]="readOnly"
            [disabled]="!!spec.disabled"
            [min]="spec.min ? (spec.min | tuiMapper: getLimit)[0] : min"
            [max]="spec.max ? (spec.max | tuiMapper: getLimit)[0] : max"
            [(ngModel)]="value"
            (blur)="control.onTouched()"
          />
        }
        @case ('datetime-local') {
          <input
            tuiInputDateTime
            type="datetime-local"
            [readonly]="readOnly"
            [disabled]="!!spec.disabled"
            [min]="spec.min ? (spec.min | tuiMapper: getLimit)[0] : min"
            [max]="spec.max ? (spec.max | tuiMapper: getLimit)[0] : max"
            [(ngModel)]="value"
            (blur)="control.onTouched()"
          />
        }
      }
    </tui-textfield>
  `,
  imports: [
    FormsModule,
    TuiInput,
    TuiIcon,
    TuiTooltip,
    TuiInputTime,
    TuiInputDate,
    TuiMapperPipe,
    TuiInputDateTime,
    HintPipe,
  ],
})
export class FormDatetimeComponent extends Control<
  IST.ValueSpecDatetime,
  string
> {
  readonly min = TUI_FIRST_DAY
  readonly max = TUI_LAST_DAY

  getTime(value: string | null) {
    return value ? TuiTime.fromString(value) : null
  }

  getLimit(limit: string): [TuiDay, TuiTime] {
    return [
      TuiDay.jsonParse(limit.slice(0, 10)),
      limit.length === 10
        ? new TuiTime(0, 0)
        : TuiTime.fromString(limit.slice(-5)),
    ]
  }
}
