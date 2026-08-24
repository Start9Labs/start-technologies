import { Component } from '@angular/core'
import {
  i18nKey,
  i18nPipe,
  MarkdownPipe,
  SafeLinksDirective,
} from '@start9labs/shared'
import { TuiDialogContext } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { injectContext } from '@taiga-ui/polymorpheus'
import { ActionSuccessGroupComponent } from './action-success-group.component'
import { ActionSuccessSingleComponent } from './action-success-single.component'
import { ActionResponse } from './types'

@Component({
  template: `
    @if (message) {
      <div
        class="g-markdown"
        safeLinks
        [innerHTML]="message | i18n | markdown: options | dompurify"
      ></div>
    }
    @if (single) {
      <app-action-success-single [single]="single" />
    }
    @if (group) {
      <app-action-success-group [group]="group" />
    }
  `,
  imports: [
    ActionSuccessGroupComponent,
    ActionSuccessSingleComponent,
    NgDompurifyPipe,
    MarkdownPipe,
    SafeLinksDirective,
    i18nPipe,
  ],
})
export class ActionSuccessPage {
  readonly data = injectContext<TuiDialogContext<void, ActionResponse>>().data

  readonly message = this.data.message as i18nKey | null
  readonly single =
    this.data.result?.type === 'single' ? this.data.result : null
  readonly group = this.data.result?.type === 'group' ? this.data.result : null

  readonly options = { breaks: true }
}
