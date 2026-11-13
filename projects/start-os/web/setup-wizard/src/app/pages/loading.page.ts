import { Component, inject } from '@angular/core'
import {
  DialogService,
  i18nPipe,
  InitializingComponent,
  TaskService,
} from '@start9labs/shared'
import { TuiButton, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { ApiService } from '../services/api.service'
import { SetupObservation } from '../services/setup-observation'

@Component({
  template: `
    @if (observation.error(); as err) {
      <section tuiCardLarge>
        <header tuiHeader>
          <h1 tuiTitle>{{ 'Error initializing server' | i18n }}</h1>
        </header>
        <div tuiNotification appearance="negative">{{ err }}</div>
        <button tuiButton (click)="restart()">
          {{ 'Restart server' | i18n }}
        </button>
      </section>
    } @else {
      <app-initializing
        [initialSetup]="true"
        [progress]="observation.progress()"
      />
    }
  `,
  styles: `
    :host {
      max-width: unset;
      align-items: stretch;
    }

    section {
      margin: 1.5rem;
    }
  `,
  providers: [SetupObservation],
  imports: [
    InitializingComponent,
    TuiButton,
    TuiNotification,
    TuiTitle,
    TuiCardLarge,
    TuiHeader,
    i18nPipe,
  ],
})
export default class LoadingPage {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly dialog = inject(DialogService)

  protected readonly observation = inject(SetupObservation)

  async restart(): Promise<void> {
    this.tasks.run(async () => {
      await this.api.rpc.setup.restart({})
      this.dialog
        .openAlert('Wait 1-2 minutes and refresh the page', {
          label: 'Server is restarting',
        })
        .subscribe()
    })
  }
}
