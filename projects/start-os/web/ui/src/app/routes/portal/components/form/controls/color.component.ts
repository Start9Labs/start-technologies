import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { MarkdownPipe, SafeLinksDirective } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { TuiIcon } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiInputColor, TuiTooltip } from '@taiga-ui/kit'
import { HintPipe } from '../pipes/hint.pipe'
import { Control } from './control'

@Component({
  selector: 'form-color',
  template: `
    <tui-textfield
      iconStart=" "
      [tuiTextfieldCleaner]="false"
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
        placeholder="#000000"
        tuiInputColor
        [readonly]="readOnly"
        [disabled]="!!spec.disabled"
        [(ngModel)]="value"
        (blur)="control.onTouched()"
      />
      @if (spec | hint; as hint) {
        <tui-icon [tuiTooltip]="tooltip">
          <ng-template #tooltip>
            <div
              class="g-markdown"
              safeLinks
              [innerHTML]="hint | markdown | dompurify"
            ></div>
          </ng-template>
        </tui-icon>
      }
    </tui-textfield>
  `,
  styles: `
    :host ::ng-deep .tui-input-color_picker {
      position: relative;
      z-index: 2;
    }
  `,
  imports: [
    MarkdownPipe,
    NgDompurifyPipe,
    SafeLinksDirective,
    FormsModule,
    TuiInputColor,
    TuiIcon,
    TuiTooltip,
    HintPipe,
  ],
})
export class FormColorComponent extends Control<IST.ValueSpecColor, string> {}
