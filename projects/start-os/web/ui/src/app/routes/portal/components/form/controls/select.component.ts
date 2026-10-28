import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { Component, inject } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { Router, RouterLink } from '@angular/router'
import { invert } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { TuiDataList, TuiIcon, TuiInput } from '@taiga-ui/core'
import {
  TuiChevron,
  TuiFluidTypography,
  tuiFluidTypographyOptionsProvider,
  TuiSelect,
  TuiTooltip,
} from '@taiga-ui/kit'

import { Control } from './control'
import { HintPipe } from '../pipes/hint.pipe'

@Component({
  selector: 'form-select',
  template: `
    <tui-textfield
      tuiChevron
      [tuiTextfieldCleaner]="false"
      [disabledItemHandler]="disabledItemHandler"
      [invalid]="control.invalid()"
      (tuiActiveZoneChange)="!$event && control.onTouched()"
    >
      @if (spec.name) {
        <label tuiLabel>{{ spec.name }} *</label>
      }
      @if (mobile) {
        <select
          tuiSelect
          [disabled]="disabled"
          [readonly]="readOnly"
          [placeholder]="spec.name"
          [items]="items"
          [(ngModel)]="selected"
          (ngModelChange)="onChange($event)"
        ></select>
      } @else {
        <input
          tuiSelect
          [disabled]="disabled"
          [readonly]="readOnly"
          [placeholder]="spec.name"
          [(ngModel)]="selected"
        />
      }
      @if (!mobile) {
        <tui-data-list *tuiDropdown>
          @for (item of items; track item) {
            @if (inverted[item]?.startsWith('~')) {
              <a
                tuiOption
                iconEnd="@tui.arrow-right"
                tuiFluidTypography
                [disabled]="disabledItemHandler(item)"
                [routerLink]="inverted[item]?.slice(1)"
              >
                {{ item }}
              </a>
            } @else {
              <button
                tuiOption
                tuiFluidTypography
                [disabled]="disabledItemHandler(item)"
                [style.white-space]="'nowrap'"
                [value]="item"
              >
                {{ item }}
              </button>
            }
          }
        </tui-data-list>
      }
      @if (spec | hint; as hint) {
        <tui-icon [tuiTooltip]="hint" />
      }
    </tui-textfield>
  `,
  providers: [tuiFluidTypographyOptionsProvider({ max: 1 })],
  imports: [
    FormsModule,
    RouterLink,
    TuiInput,
    TuiSelect,
    TuiDataList,
    TuiFluidTypography,
    TuiIcon,
    TuiTooltip,
    HintPipe,
    TuiChevron,
  ],
})
export class FormSelectComponent extends Control<IST.ValueSpecSelect, string> {
  protected readonly router = inject(Router)
  protected readonly inverted = invert(this.spec.values)
  protected readonly mobile = inject(WA_IS_MOBILE)
  protected readonly items = Object.values(this.spec.values)
  protected readonly disabledItemHandler = (item: string) =>
    Array.isArray(this.spec.disabled) &&
    !!this.inverted[item] &&
    this.spec.disabled.includes(this.inverted[item]!)

  get disabled(): boolean {
    return typeof this.spec.disabled === 'string'
  }

  get selected(): string | null {
    return (this.value && this.spec.values[this.value]) || null
  }

  set selected(value: string | null) {
    this.value = (value && this.inverted[value]) || null
  }

  protected onChange(value: string) {
    const mapped = this.inverted[value]

    if (typeof mapped === 'string' && mapped.startsWith('~')) {
      this.router.navigate([mapped.slice(1)])
    }
  }
}
