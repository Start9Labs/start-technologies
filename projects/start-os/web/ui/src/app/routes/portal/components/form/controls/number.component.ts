import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { IST } from '@start9labs/start-core'
import { TuiIcon, TuiNumberFormat, TuiInput } from '@taiga-ui/core'
import { TuiInputNumber, TuiTooltip } from '@taiga-ui/kit'

import { Control } from './control'
import { HintPipe } from '../pipes/hint.pipe'

@Component({
  selector: 'form-number',
  template: `
    <tui-textfield
      [tuiNumberFormat]="{ precision, decimalMode: 'not-zero' }"
      [invalid]="control.invalid()"
    >
      @if (spec.name) {
        <label tuiLabel>
          {{ spec.name }}
          @if (spec.required) {
            <span>*</span>
          }
        </label>
      }
      <input
        tuiInputNumber
        [postfix]="postfix"
        [min]="spec.min"
        [max]="spec.max"
        [step]="spec.step || 0"
        [disabled]="!!spec.disabled"
        [readonly]="readOnly"
        [placeholder]="placeholder"
        [(ngModel)]="value"
        (blur)="control.onTouched()"
      />
      @if (spec | hint; as hint) {
        <tui-icon [tuiTooltip]="hint" />
      }
    </tui-textfield>
  `,
  imports: [
    FormsModule,
    TuiInput,
    TuiInputNumber,
    TuiNumberFormat,
    TuiIcon,
    TuiTooltip,
    HintPipe,
  ],
})
export class FormNumberComponent extends Control<IST.ValueSpecNumber, number> {
  get precision(): number {
    return this.spec.integer ? 0 : Infinity
  }

  get postfix(): string {
    return this.spec.units && (this.value !== null || !this.spec.placeholder)
      ? ` ${this.spec.units}`
      : ''
  }

  get placeholder(): string {
    const units = this.spec.units ? ` (${this.spec.units})` : ''

    return this.spec.placeholder ? this.spec.placeholder + units : ''
  }
}
