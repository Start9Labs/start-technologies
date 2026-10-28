import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { IST } from '@start9labs/start-core'
import { TuiIcon, TuiInput } from '@taiga-ui/core'
import { TuiTextarea, TuiTooltip } from '@taiga-ui/kit'

import { Control } from './control'
import { HintPipe } from '../pipes/hint.pipe'

@Component({
  selector: 'form-textarea',
  template: `
    <tui-textfield [invalid]="control.invalid()">
      @if (spec.name) {
        <label tuiLabel>
          {{ spec.name }}
          @if (spec.required) {
            <span>*</span>
          }
        </label>
      }
      <textarea
        placeholder="Placeholder"
        tuiTextarea
        [minRows]="spec.minRows"
        [maxRows]="spec.maxRows"
        [attr.maxLength]="spec.maxLength"
        [disabled]="!!spec.disabled"
        [readonly]="readOnly"
        [placeholder]="spec.placeholder || ''"
        [(ngModel)]="value"
        (blur)="control.onTouched()"
      ></textarea>
      @if (spec | hint; as hint) {
        <tui-icon [tuiTooltip]="hint" />
      }
    </tui-textfield>
  `,
  imports: [FormsModule, TuiInput, TuiTextarea, TuiIcon, TuiTooltip, HintPipe],
})
export class FormTextareaComponent extends Control<
  IST.ValueSpecTextarea,
  string
> {}
