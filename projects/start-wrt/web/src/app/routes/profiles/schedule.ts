import { Component, signal } from '@angular/core'
import { TuiButton, TuiDialogContext } from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { ScheduleComponent } from 'src/app/components/schedule'
import { provideHelp } from 'src/app/help/help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { ModalHelp } from 'src/app/help/modal-help'
import type { ScheduleWindow } from 'src/app/services/api/api.service'

@Component({
  template: `
    <app-schedule [style.height.rem]="30" [(windows)]="windows" />
    <footer>
      <button
        tuiButton
        type="button"
        appearance="flat"
        (click)="context.$implicit.complete()"
      >
        {{ 'Cancel' | i18n }}
      </button>
      <button tuiButton (click)="context.completeWith(windows())">
        {{ 'Save' | i18n }}
      </button>
    </footer>
  `,
  hostDirectives: [ModalHelp],
  viewProviders: [provideHelp('/profiles/blackout')],
  providers: [provideHelp('/profiles/schedule')],
  imports: [ScheduleComponent, TuiButton, i18nPipe],
})
class ProfilesScheduleComponent {
  protected readonly context =
    injectContext<TuiDialogContext<ScheduleWindow[], ScheduleWindow[]>>()

  protected readonly windows = signal(this.context.data)
}

export const PROFILE_SCHEDULE = new PolymorpheusComponent(
  ProfilesScheduleComponent,
)
