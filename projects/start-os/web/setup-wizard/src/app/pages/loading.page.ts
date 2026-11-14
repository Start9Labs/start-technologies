import { Component, inject } from '@angular/core'
import {
  DialogService,
  i18nPipe,
  InitializingComponent,
  TaskService,
} from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'
import { ApiService } from '../services/api.service'
import { SetupObservation } from '../services/setup-observation'

@Component({
  template: `
    @if (observation.error(); as err) {
      <section>
        <h1>{{ 'Error initializing server' | i18n }}</h1>
        <p>{{ err }}</p>
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
      border-radius: 0.25rem;
      padding: 1rem;
      margin: 1.5rem;
      text-align: center;
      background: #e0e0e0;
      color: #333;
      --tui-background-neutral-1: rgba(0, 0, 0, 0.1);
    }
  `,
  providers: [SetupObservation],
  imports: [InitializingComponent, TuiButton, i18nPipe],
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
