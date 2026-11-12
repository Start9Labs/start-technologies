import { inject, Service } from '@angular/core'
import { withAbortSignal } from '@start9labs/shared'
import {
  catchError,
  defer,
  EMPTY,
  filter,
  from,
  ignoreElements,
  Observable,
  repeat,
  shareReplay,
  startWith,
  switchMap,
  take,
} from 'rxjs'
import { T } from '@start9labs/start-core'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { ConnectionService } from 'src/app/services/connection.service'
import { AuthService } from 'src/app/services/auth.service'

/** Shares live metrics while authenticated consumers observe them. */
@Service()
export class MetricsService extends Observable<T.Metrics> {
  private readonly connection = inject(ConnectionService)
  private readonly api = inject(ApiService)

  private readonly metrics$ = inject(AuthService).isVerified$.pipe(
    switchMap(verified =>
      verified
        ? withAbortSignal(signal =>
            defer(() =>
              this.api.streams.server.metrics.follow({}, { signal }),
            ).pipe(
              switchMap(({ initial, items }) =>
                from(items).pipe(startWith(initial.metrics)),
              ),
            ),
          ).pipe(
            catchError(() =>
              this.connection.pipe(filter(Boolean), take(1), ignoreElements()),
            ),
            repeat(),
          )
        : EMPTY,
    ),
    shareReplay({ bufferSize: 1, refCount: true }),
  )

  constructor() {
    super(subscriber => this.metrics$.subscribe(subscriber))
  }
}
