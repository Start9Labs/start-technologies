import { Component, inject } from '@angular/core'
import { FormsModule, NgModel } from '@angular/forms'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { TuiDialogContext, TuiIcon, TuiTitle } from '@taiga-ui/core'
import { TuiSwitch, TuiTooltip } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext } from '@taiga-ui/polymorpheus'

import { PushService } from 'src/app/services/push.service'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">
        {{ 'Notification settings' | i18n }}
      </h2>
    </header>
    <label class="g-toggle">
      {{ 'Web Push' | i18n }}
      <tui-icon [tuiTooltip]="hint" />
      <ng-template #hint>
        {{
          'Receive notifications in this browser while the StartOS UI is closed.'
            | i18n
        }}
        @if (!push.available) {
          <br />
          <br />
          {{ 'Disabled' | i18n }}: {{ 'Browser push unavailable' | i18n }}
        }
      </ng-template>
      <input
        #control="ngModel"
        tuiSwitch
        type="checkbox"
        size="m"
        [showIcons]="false"
        [disabled]="!push.available || !push.ready()"
        [ngModel]="push.enabled()"
        (ngModelChange)="toggle(control)"
      />
    </label>
  `,
  imports: [
    FormsModule,
    i18nPipe,
    TuiHeader,
    TuiIcon,
    TuiSwitch,
    TuiTitle,
    TuiTooltip,
  ],
})
export class NotificationSettings {
  private readonly tasks = inject(TaskService)

  protected readonly context = injectContext<TuiDialogContext<void>>()
  protected readonly push = inject(PushService)

  protected async toggle(control: NgModel) {
    await this.tasks.run(
      () => (this.push.enabled() ? this.push.disable() : this.push.enable()),
      'Saving',
    )
    control.control.setValue(this.push.enabled(), {
      emitViewToModelChange: false,
    })
  }
}
