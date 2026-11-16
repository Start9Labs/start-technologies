import { inject, Injectable } from '@angular/core'
import { merge, Observable } from 'rxjs'
import {
  distinctUntilChanged,
  filter,
  map,
  share,
  switchMap,
} from 'rxjs/operators'
import { PatchDB } from 'patch-db-client'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { ConnectionService } from 'src/app/services/connection.service'
import { OSService } from 'src/app/services/os.service'
import { PatchDbCache } from './patch-db/patch-db-cache'
import { PATCH_CACHE } from './patch-db/patch-db-source'

@Injectable({
  providedIn: 'root',
})
export class PatchDataService extends Observable<void> {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly os = inject(OSService)
  private readonly cache = inject(PatchDbCache)
  private readonly dump$ = inject(PATCH_CACHE)
  private readonly stream$ = inject(ConnectionService).pipe(
    filter(Boolean),
    switchMap(() =>
      merge(
        this.dump$.pipe(
          filter(({ id }) => !!id),
          map(dump => {
            this.cache.write(dump)
          }),
        ),
        this.patch.watch$('serverInfo', 'osRegistry').pipe(
          distinctUntilChanged(),
          map(registry => {
            this.os.loadOS(registry).catch(console.error)
          }),
        ),
      ),
    ),
    share(),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
