import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { MarkdownPipe, SafeLinksDirective } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { TuiIcon } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiSwitch, TuiTooltip } from '@taiga-ui/kit'

import { Control } from './control'
import { HintPipe } from '../pipes/hint.pipe'

@Component({
  selector: 'form-toggle',
  template: `
    {{ spec.name }}
    @if (spec.description || spec.disabled) {
      <tui-icon [tuiTooltip]="tooltip">
        <ng-template #tooltip>
          <div
            class="g-markdown"
            safeLinks
            [innerHTML]="spec | hint | markdown | dompurify"
          ></div>
        </ng-template>
      </tui-icon>
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
  imports: [
    MarkdownPipe,
    NgDompurifyPipe,
    SafeLinksDirective,
    TuiIcon,
    TuiTooltip,
    HintPipe,
    TuiSwitch,
    FormsModule,
  ],
})
export class FormToggleComponent extends Control<
  IST.ValueSpecToggle,
  boolean
> {}
