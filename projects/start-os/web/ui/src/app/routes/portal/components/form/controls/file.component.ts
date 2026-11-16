import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { TuiButton, TuiIcon } from '@taiga-ui/core'
import { TuiChip, TuiFileLike, TuiFiles, TuiTooltip } from '@taiga-ui/kit'

import { Control } from './control'

@Component({
  selector: 'form-file',
  template: `
    <label tuiInputFiles>
      <input
        tuiInputFiles
        [invalid]="control.invalid()"
        [accept]="spec.extensions.join(',')"
        [maxFileSize]="spec.maxSize ?? unlimited"
        [(ngModel)]="value"
        (blur)="control.onTouched()"
      />
      <ng-template let-drop>
        <div class="template" [class.template_hidden]="drop">
          <div class="label">
            {{ spec.name }}
            @if (spec.required) {
              <span>*</span>
            }
            @if (spec.description) {
              <tui-icon [tuiTooltip]="spec.description" />
            }
          </div>
          @if (value) {
            <span tuiChip>
              {{ value.name }}
              <button
                tuiIconButton
                type="button"
                appearance="icon"
                size="xs"
                iconStart="@tui.x"
                (click.stop)="value = null"
              >
                {{ 'Delete' | i18n }}
              </button>
            </span>
          } @else {
            <small>{{ 'Click or drop file here' | i18n }}</small>
          }
        </div>
        <div class="drop" [class.drop_hidden]="!drop">
          {{ 'Drop file here' | i18n }}
        </div>
      </ng-template>
    </label>
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    .template {
      @include taiga.transition(opacity);

      width: 100%;
      display: flex;
      align-items: center;
      padding: 0 0.5rem;
      font: var(--tui-typography-body-m);
      font-weight: bold;

      &_hidden {
        opacity: 0;
      }
    }

    .drop {
      @include taiga.fullsize();
      @include taiga.transition(opacity);
      display: flex;
      align-items: center;
      justify-content: space-around;

      &_hidden {
        opacity: 0;
      }
    }

    .label {
      display: flex;
      align-items: center;
      max-width: 50%;
    }

    small {
      max-width: 50%;
      font-weight: normal;
      color: var(--tui-text-secondary);
      margin-left: auto;
    }

    tui-chip {
      z-index: 1;
      margin: -0.25rem -0.25rem -0.25rem auto;
      pointer-events: auto;
    }
  `,
  imports: [
    FormsModule,
    TuiFiles,
    TuiIcon,
    TuiTooltip,
    TuiChip,
    TuiButton,
    i18nPipe,
  ],
})
export class FormFileComponent extends Control<IST.ValueSpecFile, TuiFileLike> {
  protected readonly unlimited = Infinity
}
