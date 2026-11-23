import { inject, Injectable } from '@angular/core'
import { EMPTY, merge, Observable, timer } from 'rxjs'
import { filter, finalize, map, share, switchMap } from 'rxjs/operators'
import { ConnectionService } from 'src/app/services/connection.service'
import { PatchDbCache } from './patch-db/patch-db-cache'
import { PATCH_CACHE } from './patch-db/patch-db-source'

@Injectable({
  providedIn: 'root',
})
export class PatchDataService extends Observable<void> {
  private readonly cache = inject(PatchDbCache)
  private readonly dump$ = inject(PATCH_CACHE)
  private readonly stream$ = inject(ConnectionService).pipe(
    switchMap(live =>
      live
        ? merge(
            this.dump$.pipe(
              filter(({ id }) => !!id),
              map(dump => {
                this.cache.write(dump)
              }),
            ),
            timer(0, 30_000).pipe(
              map(() => {
                this.cache.markSynced()
              }),
            ),
          ).pipe(finalize(() => this.cache.markSynced()))
        : EMPTY,
    ),
    share(),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
