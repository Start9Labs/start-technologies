import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { Router } from '@angular/router'
import {
  DialogService,
  formatProgress,
  getErrorMessage,
  i18nPipe,
  InitializingComponent,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'
import {
  catchError,
  filter,
  from,
  map,
  startWith,
  switchMap,
  tap,
  timer,
} from 'rxjs'
import { ApiService } from '../services/api.service'
import { StateService } from '../services/state.service'

@Component({
  template: `
    @if (error(); as err) {
      <section>
        <h1>{{ 'Error initializing server' | i18n }}</h1>
        <p>{{ err }}</p>
        <button tuiButton (click)="restart()">
          {{ 'Restart server' | i18n }}
        </button>
      </section>
    } @else {
      <app-initializing [initialSetup]="true" [progress]="progress()" />
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
  imports: [InitializingComponent, TuiButton, i18nPipe],
})
export default class LoadingPage {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly dialog = inject(DialogService)
  private readonly router = inject(Router)

  readonly type = inject(StateService).setupType
  readonly progress = toSignal(
    from(this.getStatus()).pipe(
      filter(Boolean),
      switchMap(({ guid, progress }) =>
        this.api.openWebsocket$<T.FullProgress>(guid).pipe(
          startWith(progress),
          tap(({ overall }) => {
            if (overall === true) {
              this.getStatus()
            }
          }),
        ),
      ),
      map(formatProgress),
      catchError((_, caught$) => timer(500).pipe(switchMap(() => caught$))),
    ),
    { initialValue: { total: 0, message: '' } },
  )

  error = signal('')

  private async getStatus(): Promise<{
    status: 'running'
    guid: string
    progress: T.FullProgress
  } | null> {
    try {
      const res = await this.api.rpc.setup.status({})

      if (res.status === 'running') {
        return res
      } else if (res.status === 'complete') {
        this.router.navigate(['/success'])
      } else {
        // incomplete or needs-install - shouldn't happen on loading page
        this.router.navigate(['/language'])
      }
    } catch (e: any) {
      this.error.set(getErrorMessage(e))
    }

    return null
  }

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
