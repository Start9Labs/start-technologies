import { Component, inject } from '@angular/core'
import { ScheduleComponent } from 'src/app/components/schedule'
import { provideHelp } from 'src/app/help/help'
import { BlackoutService } from './service'

@Component({
  template: `
    <app-schedule
      [windows]="service.data() || []"
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
}
