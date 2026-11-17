import { inject, Injectable } from '@angular/core'
import { Observable } from 'rxjs'
import { filter, map, share, switchMap } from 'rxjs/operators'
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
    filter(Boolean),
    switchMap(() =>
      this.dump$.pipe(
        filter(({ id }) => !!id),
        map(dump => {
          this.cache.write(dump)
        }),
      ),
    ),
    share(),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
