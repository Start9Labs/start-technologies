import { Component } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { TuiIcon } from '@taiga-ui/core'
import { TuiSegmented, TuiTooltip } from '@taiga-ui/kit'

import { Control } from './control'
import { HintPipe } from '../pipes/hint.pipe'

@Component({
  selector: 'form-tristate',
  template: `
    {{ spec.name }}
    @if (spec.description || spec.disabled) {
      <tui-icon [tuiTooltip]="tooltip" />
      <ng-template #tooltip>{{ spec | hint }}</ng-template>
    }
    <tui-segmented
      size="s"
      [activeItemIndex]="activeIndex"
      (click)="control.onTouched()"
    >
      <button
        type="button"
        class="off"
        [attr.aria-label]="'Off' | i18n"
        [title]="'Off' | i18n"
        [disabled]="!!spec.disabled || readOnly"
        (click)="select(false)"
      >
        <tui-icon icon="@tui.x" />
      </button>
      <button
        type="button"
        class="default"
        [attr.aria-label]="'Default' | i18n"
        [title]="'Default' | i18n"
        [disabled]="!!spec.disabled || readOnly"
        (click)="select(null)"
      >
        <tui-icon icon="@tui.minus" />
      </button>
      <button
        type="button"
        class="on"
        [attr.aria-label]="'On' | i18n"
        [title]="'On' | i18n"
        [disabled]="!!spec.disabled || readOnly"
        (click)="select(true)"
      >
        <tui-icon icon="@tui.check" />
      </button>
    </tui-segmented>
  `,
  styles: `
    tui-segmented {
      margin-left: auto;
    }
    .off tui-icon {
      color: var(--tui-status-negative);
    }
    .default tui-icon {
      color: var(--tui-text-tertiary);
    }
    .on tui-icon {
      color: var(--tui-status-positive);
    }
  `,
  host: { class: 'g-toggle' },
  imports: [TuiIcon, TuiTooltip, HintPipe, TuiSegmented, i18nPipe],
})
export class FormTriStateComponent extends Control<
  IST.ValueSpecTriState,
  boolean | null
> {
  get activeIndex(): number {
    if (this.value === false) return 0
    if (this.value === true) return 2
    return 1
  }

  select(next: boolean | null) {
    this.value = next
  }
}
