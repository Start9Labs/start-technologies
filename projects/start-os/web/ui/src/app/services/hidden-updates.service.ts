import { inject, Injectable } from '@angular/core'
import { ErrorService } from '@start9labs/shared'
import { PatchDB, pathFromArray } from 'patch-db-client'
import {
  BehaviorSubject,
  combineLatest,
  firstValueFrom,
  map,
  Observable,
  shareReplay,
} from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

// Singleton that owns the "hidden updates" feature: it exposes the merged
// (persisted + session-optimistic) hidden map and the `hide()` action.
// Keeping this in a service — rather than in UpdatesComponent — lets the
// navbar's updates badge stay in sync with the in-page row state, so hiding
// an update decrements both counts on the same change-detection tick.
@Injectable({
  providedIn: 'root',
})
export class HiddenUpdatesService {
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly errorService = inject(ErrorService)

  // Session-local optimistic state. Grows monotonically on successful writes
  // (patchdb eventually reflects them) and rolls back individual entries on
  // failed writes. Never cleared on success — clearing between setDbValue
  // resolving and patchdb's watch echoing the new persisted value leaves a
  // window where neither source contains the version and the row flashes
  // back into view.
  private readonly optimistic$ = new BehaviorSubject<Record<string, string[]>>(
    {},
  )

  readonly effective$: Observable<Record<string, string[]>> = combineLatest([
    this.patch.watch$('ui', 'hiddenUpdates'),
    this.optimistic$,
  ]).pipe(
    map(([persisted, optimistic]) => merge(persisted, optimistic)),
    shareReplay({ bufferSize: 1, refCount: true }),
  )

  hide(id: string, version: string): void {
    const opt = this.optimistic$.value
    this.optimistic$.next({
      ...opt,
      [id]: [...(opt[id] || []), version],
    })

    // Re-read optimistic$.value *at write time* and merge with persisted so
    // concurrent hides accumulate instead of clobbering each other. Without
    // this, two rapid hides both read persisted=[], then the second write
    // wins and drops the first version.
    firstValueFrom(this.patch.watch$('ui', 'hiddenUpdates'))
      .then(persisted => {
        const latestOpt = this.optimistic$.value[id] || []
        const merged = Array.from(
          new Set([...(persisted[id] || []), ...latestOpt]),
        )
        return this.api.rpc.db.put.ui({
          pointer: pathFromArray(['hiddenUpdates', id]),
          value: merged,
        })
      })
      .catch(e => {
        const current = this.optimistic$.value
        const next = { ...current }
        const remaining = (next[id] || []).filter(v => v !== version)
        if (remaining.length === 0) delete next[id]
        else next[id] = remaining
        this.optimistic$.next(next)
        this.errorService.handleError(e)
      })
  }
}

function merge(
  a: Record<string, string[]>,
  b: Record<string, string[]>,
): Record<string, string[]> {
  const out: Record<string, string[]> = { ...a }
  for (const [id, versions] of Object.entries(b)) {
    out[id] = Array.from(new Set([...(out[id] || []), ...versions]))
  }
  return out
}
