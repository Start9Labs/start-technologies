import { Component, computed, input } from '@angular/core'
import { MarkdownPipe, SafeLinksDirective } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiTitle } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'

interface ActionItem {
  readonly name: string
  readonly description: string
  readonly icon?: string
  readonly visibility?: T.ActionVisibility
}

@Component({
  selector: '[action]',
  template: `
    <div tuiTitle>
      <strong>{{ action().name }}</strong>
      <div
        tuiSubtitle
        class="g-markdown"
        safeLinks
        [innerHTML]="action().description | markdown | dompurify"
      ></div>
      @if (disabled()) {
        <div tuiSubtitle class="g-warning">{{ disabled() }}</div>
      }
    </div>
  `,
  imports: [MarkdownPipe, NgDompurifyPipe, SafeLinksDirective, TuiTitle],
  host: {
    '[attr.disabled]': '(!!disabled() || inactive()) || null',
  },
})
export class ServiceActionComponent {
  action = input.required<ActionItem>()
  inactive = input.required<boolean>()

  disabled = computed(
    (action = this.action()) =>
      typeof action.visibility === 'object' && action.visibility.disabled,
  )
}
