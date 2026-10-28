import { Component, inject, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import {
  getAllKeyboardsSorted,
  i18nPipe,
  Keyboard,
  LanguageCode,
} from '@start9labs/shared'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { TuiButton, TuiTitle } from '@taiga-ui/core'
import {
  TuiButtonLoading,
  TuiChevron,
  TuiDataListWrapper,
  TuiSelect,
} from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { ApiService } from '../services/api.service'
import { StateService } from '../services/state.service'

@Component({
  template: `
    <section tuiCardLarge="compact">
      <header tuiHeader>
        <h2 tuiTitle>{{ 'Select Keyboard Layout' | i18n }}</h2>
      </header>
      <tui-textfield
        tuiChevron
        [stringify]="stringify"
        [tuiTextfieldCleaner]="false"
      >
        <label tuiLabel>{{ 'Keyboard' | i18n }}</label>
        @if (mobile) {
          <select tuiSelect [(ngModel)]="selected" [items]="keyboards"></select>
        } @else {
          <input tuiSelect [(ngModel)]="selected" />
        }
        @if (!mobile) {
          <tui-data-list-wrapper *tuiDropdown [items]="keyboards" />
        }
      </tui-textfield>

      <footer>
        <button
          tuiButton
          [disabled]="!selected"
          [loading]="saving()"
          (click)="continue()"
        >
          {{ 'Continue' | i18n }}
        </button>
      </footer>
    </section>
  `,
  imports: [
    FormsModule,
    TuiCardLarge,
    TuiButton,
    TuiButtonLoading,
    TuiChevron,
    TuiSelect,
    TuiDataListWrapper,
    TuiHeader,
    TuiTitle,
    i18nPipe,
  ],
})
export default class KeyboardPage {
  private readonly api = inject(ApiService)
  private readonly stateService = inject(StateService)

  protected readonly mobile = inject(WA_IS_MOBILE)
  // All keyboards, with language-specific keyboards at the top
  readonly keyboards = getAllKeyboardsSorted(
    this.stateService.language as LanguageCode,
  )
  selected =
    this.keyboards.find(k => k.layout === this.stateService.keyboard) ||
    this.keyboards[0]!

  readonly saving = signal(false)

  readonly stringify = (kb: Keyboard) => kb.name

  async continue() {
    this.saving.set(true)

    try {
      // Send keyboard to backend
      await this.api.rpc.setup.setKeyboard({
        layout: this.selected.layout,
        keymap: this.selected.keymap,
        model: null,
        variant: null,
        options: [],
      })

      this.stateService.keyboard = this.selected.layout
      await this.stateService.navigateAfterLocale()
    } finally {
      this.saving.set(false)
    }
  }
}
