import { inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { getErrorMessage } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  catchError,
  exhaustMap,
  from,
  map,
  of,
  scan,
  startWith,
  switchMap,
  timer,
} from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

interface StatusState {
  status: T.NutStatus | null
  error: string | null
  refreshing: boolean
}

const EMPTY_STATE: StatusState = {
  status: null,
  error: null,
  refreshing: false,
}

@Injectable()
export class NutStatusService {
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  readonly state = toSignal(
    this.patch.watch$('serverInfo', 'nut').pipe(
      switchMap(config =>
        !config?.enabled || !config.settings
          ? of(EMPTY_STATE)
          : timer(0, 5000).pipe(
              exhaustMap(() =>
                from(this.api.rpc.server.nutStatus({})).pipe(
                  map(status => ({ status, error: null, refreshing: false })),
                  catchError(error =>
                    of({
                      status: null,
                      error: getErrorMessage(error),
                      refreshing: false,
                    }),
                  ),
                  startWith({ refreshing: true }),
                ),
              ),
              scan(
                (state: StatusState, update: Partial<StatusState>) => ({
                  ...state,
                  ...update,
                }),
                EMPTY_STATE,
              ),
              startWith(EMPTY_STATE),
            ),
      ),
    ),
    { initialValue: EMPTY_STATE },
  )
}
