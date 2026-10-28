import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  formatProgress,
  InitializingComponent,
  provideSetupLogsService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  catchError,
  defer,
  from,
  map,
  startWith,
  switchMap,
  tap,
  timer,
} from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { StateService } from 'src/app/services/state.service'

@Component({
  template: '<app-initializing [progress]="progress()" />',
  providers: [
    provideSetupLogsService(ApiService, api => api.rpc.init.logs.follow({})),
  ],
  styles: ':host { height: 100%; }',
  imports: [InitializingComponent],
})
export default class InitializingPage {
  private readonly api = inject(ApiService)
  private readonly state = inject(StateService)

  readonly progress = toSignal(
    defer(() => from(this.api.rpc.init.subscribe({}))).pipe(
      switchMap(({ guid, progress }) =>
        this.api
          .openWebsocket$<T.FullProgress>(guid, {
            closeObserver: {
              next: () => {
                this.state.retrigger(true, 250)
              },
            },
          })
          .pipe(startWith(progress)),
      ),
      map(formatProgress),
      catchError((_, caught$) => {
        return timer(500).pipe(switchMap(() => caught$))
      }),
    ),
    { initialValue: { total: 0, message: 'waiting...' } },
  )
}
