import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { IST } from '@start9labs/start-core'
import { TuiIcon } from '@taiga-ui/core'
import { TuiSwitch, TuiTooltip } from '@taiga-ui/kit'

import { Control } from './control'
import { HintPipe } from '../pipes/hint.pipe'

@Component({
  selector: 'form-toggle',
  template: `
    {{ spec.name }}
    @if (spec.description || spec.disabled) {
      <tui-icon [tuiTooltip]="tooltip" />
      <ng-template #tooltip>{{ spec | hint }}</ng-template>
    }
    <input
      tuiSwitch
      type="checkbox"
      size="m"
      [disabled]="!!spec.disabled || readOnly"
      [showIcons]="false"
      [(ngModel)]="value"
      (blur)="control.onTouched()"
    />
  `,
  host: { class: 'g-toggle' },
  imports: [TuiIcon, TuiTooltip, HintPipe, TuiSwitch, FormsModule],
})
export class FormToggleComponent extends Control<
  IST.ValueSpecToggle,
  boolean
> {}
