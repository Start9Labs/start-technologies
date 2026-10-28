import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import { IST, utils } from '@start9labs/start-core'
import { tuiInjectElement } from '@taiga-ui/cdk'
import { TuiButton, TuiIcon, TuiInput } from '@taiga-ui/core'
import { TuiTooltip } from '@taiga-ui/kit'

import { Control } from './control'
import { HintPipe } from '../pipes/hint.pipe'

@Component({
  selector: 'form-text',
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
      <input
        tuiInput
        [attr.inputmode]="spec.inputmode"
        [attr.minLength]="spec.minLength"
        [attr.maxLength]="spec.maxLength"
        [style.-webkit-text-security]="spec.masked && masked ? 'disc' : null"
        [placeholder]="spec.placeholder || ''"
        [disabled]="!!spec.disabled"
        [readonly]="readOnly"
        [(ngModel)]="value"
        (blur)="control.onTouched()"
      />
      @if (spec.generate) {
        <button
          tuiIconButton
          type="button"
          appearance="icon"
          title="Generate"
          size="xs"
          iconStart="@tui.refresh-ccw"
          (click)="generate()"
        ></button>
      }
      @if (spec.masked) {
        <button
          tuiIconButton
          type="button"
          appearance="icon"
          title="Toggle masking"
          size="xs"
          [iconStart]="masked ? '@tui.eye' : '@tui.eye-off'"
          (click)="masked = !masked"
        >
          {{ 'Reveal/Hide' | i18n }}
        </button>
      }
      <button
        tuiIconButton
        type="button"
        iconStart="@tui.trash"
        appearance="icon"
        size="xs"
        title="Remove"
        class="remove"
        (pointerdown.prevent)="(0)"
        (click)="remove()"
      ></button>
      @if (spec | hint; as hint) {
        <tui-icon [tuiTooltip]="hint" />
      }
    </tui-textfield>
  `,
  styles: `
    .remove {
      display: none;
      order: 1;
    }

    :host-context(form-control.array > :host) .remove {
      display: flex;
    }
  `,
  imports: [
    FormsModule,
    TuiInput,
    TuiButton,
    TuiIcon,
    TuiTooltip,
    HintPipe,
    i18nPipe,
  ],
})
export class FormTextComponent extends Control<IST.ValueSpecText, string> {
  private readonly el = tuiInjectElement()

  masked = true

  generate() {
    this.value = utils.getDefaultString(this.spec.generate || '')
  }

  remove() {
    this.el.dispatchEvent(new CustomEvent('remove', { bubbles: true }))
  }
}
