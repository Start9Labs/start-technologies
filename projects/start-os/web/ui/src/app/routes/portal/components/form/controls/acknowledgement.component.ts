import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { MarkdownPipe, SafeLinksDirective } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { TuiCheckbox, TuiIcon, TuiLabel } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiTooltip } from '@taiga-ui/kit'

import { Control } from './control'
import { HintPipe } from '../pipes/hint.pipe'

@Component({
  selector: 'form-acknowledgement',
  template: `
    <label tuiLabel>
      <input
        tuiCheckbox
        type="checkbox"
        [(ngModel)]="value"
        (blur)="control.onTouched()"
      />
      {{ spec.name }}
      @if (spec.description) {
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
    </label>
  `,
  imports: [
    MarkdownPipe,
    NgDompurifyPipe,
    SafeLinksDirective,
    TuiLabel,
    TuiCheckbox,
    TuiIcon,
    TuiTooltip,
    HintPipe,
    FormsModule,
  ],
})
export class FormAcknowledgementComponent extends Control<
  IST.ValueSpecAcknowledgement,
  boolean
> {}
