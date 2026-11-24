import { Component, computed, input } from '@angular/core'
import { T } from '@start9labs/start-core'
import { TuiTitle } from '@taiga-ui/core'

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
      <div tuiSubtitle>{{ action().description }}</div>
      @if (disabled()) {
        <div tuiSubtitle class="g-warning">{{ disabled() }}</div>
      }
    </div>
  `,
  imports: [TuiTitle],
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
