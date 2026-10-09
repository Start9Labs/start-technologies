import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { TuiButton, TuiDialogContext, TuiInput } from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import { i18nPipe } from '../i18n/i18n.pipe'
import { i18nKey } from '../i18n/i18n.providers'

@Component({
  template: `
    <p>{{ options.message }}</p>
    @if (options.warning) {
      <p class="warning">{{ options.warning }}</p>
    }
    <tui-textfield>
      @if (options.label) {
        <label tuiLabel>
          {{ options.label }}
          @if (options.required !== false && options.label) {
            <span>*</span>
          }
        </label>
      }
      <input
        tuiInput
        autocapitalize="off"
        [(ngModel)]="value"
        [placeholder]="options.placeholder || ''"
        [type]="options.useMask && masked ? 'password' : 'text'"
        [autocomplete]="options.useMask ? 'off' : ''"
        (keyup.enter)="submit(value.trim())"
      />
      @if (options.useMask) {
        <button
          tuiIconButton
          type="button"
          size="xs"
          appearance="icon"
          [iconStart]="masked ? '@tui.eye' : '@tui.eye-off'"
          (click)="masked = !masked"
        >
          {{ (masked ? 'Show password' : 'Hide password') | i18n }}
        </button>
      }
    </tui-textfield>
    @if (error) {
      <p class="error">{{ error }}</p>
    }
    <footer class="g-buttons">
      <button tuiButton type="button" appearance="secondary" (click)="cancel()">
        {{ 'Cancel' | i18n }}
      </button>
      <button
        tuiButton
        type="button"
        [disabled]="!value && options.required !== false"
        (click)="submit(value.trim())"
      >
        {{ options.buttonText || ('Submit' | i18n) }}
      </button>
    </footer>
  `,
  styles: `
    .warning {
      color: var(--tui-status-warning);
    }

    .error {
      color: var(--tui-status-negative);
    }
  `,
  imports: [FormsModule, TuiButton, TuiInput, i18nPipe],
})
export class PromptModal {
  private readonly context =
    injectContext<TuiDialogContext<string, PromptOptions>>()

  value = this.options.initialValue || ''
  error = ''
  masked = true

  get options(): PromptOptions {
    return this.context.data
  }

  cancel() {
    this.context.$implicit.complete()
  }

  submit(value: string) {
    if (value || !this.options.required) {
      const { pattern, patternError } = this.options
      if (pattern && !new RegExp(pattern).test(value)) {
        this.error = patternError || 'Invalid input'
        return
      }
      this.error = ''
      this.context.$implicit.next(value)
    }
  }
}

export const PROMPT = new PolymorpheusComponent(PromptModal)

export interface PromptOptions {
  message: i18nKey
  label?: i18nKey
  warning?: i18nKey
  buttonText?: i18nKey
  placeholder?: i18nKey
  required?: boolean
  useMask?: boolean
  initialValue?: string | null
  pattern?: string
  patternError?: string
}
