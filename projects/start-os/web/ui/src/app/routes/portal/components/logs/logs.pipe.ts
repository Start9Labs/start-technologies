import { inject, Pipe, PipeTransform } from '@angular/core'
import {
  convertAnsi,
  i18nPipe,
  toLocalIsoString,
  withAbortSignal,
} from '@start9labs/shared'
import {
  bufferTime,
  catchError,
  concat,
  concatWith,
  defer,
  delay,
  EMPTY,
  filter,
  from,
  ignoreElements,
  map,
  merge,
  Observable,
  of,
  repeat,
  scan,
  skipWhile,
  switchMap,
  take,
  tap,
  throwError,
} from 'rxjs'
import { FollowLogs } from 'src/app/services/api/api.types'
import { ConnectionService } from 'src/app/services/connection.service'
import { LogsComponent } from './logs.component'

@Pipe({
  name: 'logs',
})
export class LogsPipe implements PipeTransform {
  private readonly logs = inject(LogsComponent)
  private readonly connection = inject(ConnectionService)
  private readonly i18n = inject(i18nPipe)

  transform(followLogs: FollowLogs): Observable<readonly string[]> {
    return merge(
      this.logs.status$.pipe(
        skipWhile(value => value === 'connected'),
        filter(value => value === 'connected'),
        map(() => this.getMessage(true)),
      ),
      withAbortSignal(signal =>
        defer(() => followLogs(this.options, { signal })).pipe(
          tap(({ initial }) => {
            if (initial.startCursor) this.logs.setCursor(initial.startCursor)
            this.logs.status$.next('connected')
          }),
          switchMap(({ items }) => from(items)),
          bufferTime(1000),
          filter(logs => !!logs.length),
          map(convertAnsi),
          concatWith(throwError(() => new Error('Log observation ended'))),
        ),
      ),
    ).pipe(
      catchError(() =>
        concat(
          this.logs.status$.value === 'connected'
            ? of(this.getMessage(false))
            : EMPTY,
          this.connection.pipe(
            tap(v =>
              this.logs.status$.next(v ? 'reconnecting' : 'disconnected'),
            ),
            filter(Boolean),
            delay(1000),
            take(1),
            ignoreElements(),
          ),
        ),
      ),
      repeat(),
      scan((logs: string[], log) => [...logs, log], []),
    )
  }

  private getMessage(success: boolean): string {
    return `<div style="color: ${
      success ? 'var(--tui-status-positive)' : 'var(--tui-status-negative)'
    }; text-align: center;">${this.i18n.transform(
      success ? 'Reconnected' : 'Disconnected',
    )} at ${toLocalIsoString(new Date())}</div>`
  }

  private get options() {
    return this.logs.status$.value === 'connected' ? { limit: 400 } : {}
  }
}
