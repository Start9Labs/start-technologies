import { Component, inject } from '@angular/core'
import { FormsModule, NgModel } from '@angular/forms'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { TuiCell, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiSwitch } from '@taiga-ui/kit'
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
    <label tuiCell>
      <span tuiTitle>
        {{ 'Push' | i18n }}
        @if (!push.available) {
          <span tuiSubtitle>{{ 'Browser push unavailable' | i18n }}</span>
        }
      </span>
      <input
        #control="ngModel"
        tuiSwitch
        type="checkbox"
        [showIcons]="false"
        [disabled]="!push.available || !push.ready()"
        [ngModel]="push.enabled()"
        (ngModelChange)="toggle(control)"
      />
    </label>
  `,
  imports: [FormsModule, i18nPipe, TuiCell, TuiHeader, TuiSwitch, TuiTitle],
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
