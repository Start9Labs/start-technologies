import { computed, inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import type { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  catchError,
  distinctUntilChanged,
  map,
  merge,
  of,
  scan,
  startWith,
  Subject,
  switchMap,
} from 'rxjs'

import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { mergeBackupHistory, readManualHistory } from './history-data'

type HistoryState = { runs: T.BackupRun[]; loading: boolean; error: boolean }
const EMPTY_HISTORY: HistoryState = { runs: [], loading: true, error: false }

type HistoryUpdate =
  | { runs: T.BackupRun[] }
  | { loading: true }
  | { error: true }

@Injectable()
export class BackupHistoryService {
  private readonly api = inject(ApiService)
  private readonly activities$ = inject<PatchDB<DataModel>>(PatchDB).watch$(
    'scheduledBackups',
    'activities',
  )
  private readonly activities = toSignal(this.activities$, { initialValue: {} })
  private readonly refresh = new Subject<void>()
  readonly reports = toSignal(
    merge(
      this.refresh,
      this.activities$.pipe(
        map(activities =>
          Object.values(activities)
            .filter(activity => activity.kind === 'manual')
            .map(activity => `${activity.id}:${activity.completedAt || ''}`)
            .sort()
            .join(','),
        ),
        distinctUntilChanged(),
      ),
    ).pipe(
      switchMap(() =>
        readManualHistory(params =>
          this.api.rpc.backup.history.list(params),
        ).pipe(
          map((runs): HistoryUpdate => ({ runs })),
          catchError(() => of<HistoryUpdate>({ error: true })),
          startWith<HistoryUpdate>({ loading: true }),
        ),
      ),
      scan<HistoryUpdate, HistoryState>(
        (state, update) =>
          'runs' in update
            ? { runs: update.runs, loading: false, error: false }
            : 'error' in update
              ? { ...state, loading: false, error: true }
              : { ...state, loading: true, error: false },
        EMPTY_HISTORY,
      ),
    ),
    { initialValue: EMPTY_HISTORY },
  )
  readonly entries = computed(() =>
    mergeBackupHistory(Object.values(this.activities()), this.reports().runs),
  )

  retry() {
    this.refresh.next()
  }
}
