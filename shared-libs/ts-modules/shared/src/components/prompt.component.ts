import { Component } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { TuiButton, TuiDialogContext, TuiIcon, TuiInput } from '@taiga-ui/core'
import { TuiPassword } from '@taiga-ui/kit'
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
        [type]="options.useMask ? 'password' : 'text'"
        [autocomplete]="options.useMask ? 'off' : ''"
        (keyup.enter)="submit(value)"
      />
      @if (options.useMask) {
        <tui-icon tuiPassword />
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
        (click)="submit(value)"
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
  imports: [FormsModule, TuiButton, TuiIcon, TuiInput, TuiPassword, i18nPipe],
})
export class PromptModal {
  private readonly context =
    injectContext<TuiDialogContext<string, PromptOptions>>()

  value = this.options.initialValue || ''
  error = ''

  get options(): PromptOptions {
    return this.context.data
  }

  cancel() {
    this.context.$implicit.complete()
  }

  submit(value: string) {
    if (!this.options.useMask) value = value.trim()
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
