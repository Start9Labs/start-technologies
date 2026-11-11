import { inject, Injectable } from '@angular/core'
import {
  catchError,
  defer,
  EMPTY,
  filter,
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
@Injectable({
  providedIn: 'root',
})
export class MetricsService extends Observable<T.Metrics> {
  private readonly connection = inject(ConnectionService)
  private readonly api = inject(ApiService)

  private readonly metrics$ = inject(AuthService).isVerified$.pipe(
    switchMap(verified =>
      verified
        ? defer(() => this.api.rpc.server.metrics.follow({})).pipe(
            switchMap(({ guid, metrics }) =>
              this.api.openWebsocket$<T.Metrics>(guid).pipe(startWith(metrics)),
            ),
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
