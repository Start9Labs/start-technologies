import { Component, inject } from '@angular/core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TUI_CONFIRM } from '@taiga-ui/kit'
import { firstValueFrom } from 'rxjs'
import { ScheduleComponent } from 'src/app/components/schedule'
import { provideHelp } from 'src/app/help/help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { ScheduleWindow } from 'src/app/services/api/api.service'
import { BlackoutService } from './service'

@Component({
  template: `
    <app-schedule
      [windows]="service.data() || []"
      [approve]="approve"
      (windowsChange)="service.store($event)"
    />
  `,
  styles: `
    :host {
      max-width: 50rem;
      margin-bottom: -5rem;
    }
  `,
  providers: [provideHelp('/wifi/blackout-schedule/dialog')],
  host: { class: 'g-page' },
  imports: [ScheduleComponent],
})
export default class BlackoutScheduleComponent {
  protected readonly service = inject(BlackoutService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)

  protected readonly approve = async (windows: ScheduleWindow[]) => {
    try {
      if (!(await this.service.preview(windows))) return true
    } catch {
      return false
    }
    return firstValueFrom(
      this.dialogs.open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform('Turn off Wi-Fi now?'),
        data: {
          content: this.i18n.transform(
            'A blackout window covers the current time, so saving turns Wi-Fi off immediately for every device on it, including this one if it is connected over Wi-Fi.',
          ),
          yes: this.i18n.transform('Turn off Wi-Fi'),
          no: this.i18n.transform('Cancel'),
        },
      }),
      { defaultValue: false },
    )
  }
}
