import { Component, inject } from '@angular/core'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'

import { PushService } from 'src/app/services/push.service'

@Component({
  selector: 'button[browserPush]',
  template: `
    @if (!push.available) {
      {{ 'Browser push unavailable' | i18n }}
    } @else if (push.enabled()) {
      {{ 'Disable browser push' | i18n }}
    } @else {
      {{ 'Enable browser push' | i18n }}
    }
  `,
  host: {
    tuiButton: '',
    type: 'button',
    '[disabled]': '!push.available || !push.ready()',
    '(click)': 'toggle()',
  },
  hostDirectives: [TuiButton],
  imports: [i18nPipe],
})
export class BrowserPush {
  private readonly tasks = inject(TaskService)
  protected readonly push = inject(PushService)

  protected toggle() {
    this.tasks.run(
      () => (this.push.enabled() ? this.push.disable() : this.push.enable()),
      'Saving',
    )
  }
}
