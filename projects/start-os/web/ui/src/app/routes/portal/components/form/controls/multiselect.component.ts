import { Component, computed } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { invert, MarkdownPipe, SafeLinksDirective } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { TuiIcon, TuiTextfield } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiChevron, TuiMultiSelect, TuiTooltip } from '@taiga-ui/kit'
import { HintPipe } from '../pipes/hint.pipe'
import { Control } from './control'

@Component({
  selector: 'form-multiselect',
  template: `
    <tui-textfield
      multi
      tuiChevron
      [disabledItemHandler]="disabledItemHandler"
      [invalid]="control.invalid()"
    >
      @if (spec.name) {
        <label tuiLabel>{{ spec.name }}</label>
      }
      <select
        tuiMultiSelect
        [disabled]="disabled"
        [readonly]="readOnly"
        [items]="items"
        [ngModel]="selected()"
        (ngModelChange)="onSelected($event)"
        (blur)="control.onTouched()"
      ></select>
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
  imports: [
    MarkdownPipe,
    NgDompurifyPipe,
    SafeLinksDirective,
    FormsModule,
    TuiTextfield,
    TuiMultiSelect,
    TuiIcon,
    TuiTooltip,
    HintPipe,
    TuiChevron,
  ],
})
export class FormMultiselectComponent extends Control<
  IST.ValueSpecMultiselect,
  readonly string[]
> {
  private readonly inverted = invert(this.spec.values)

  private readonly isDisabled = (item: string) =>
    Array.isArray(this.spec.disabled) &&
    !!this.inverted[item] &&
    this.spec.disabled.includes(this.inverted[item]!)

  private readonly isExceedingLimit = (item: string) =>
    !!this.spec.maxLength &&
    this.selected().length >= this.spec.maxLength &&
    !this.selected().includes(item)

  readonly disabledItemHandler = (item: string): boolean =>
    this.isDisabled(item) || this.isExceedingLimit(item)

  readonly items = Object.values(this.spec.values)
  readonly selected = computed(
    () =>
      this.control.value().map((key: string) => this.spec.values[key] || '') ||
      [],
  )

  get disabled(): boolean {
    return typeof this.spec.disabled === 'string'
  }

  onSelected(value: string[]) {
    this.value = Object.entries(this.spec.values)
      .filter(([_, v]) => value.includes(v))
      .map(([k]) => k)
  }
}
