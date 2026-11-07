import { inject, Injectable } from '@angular/core'
import { Observable } from 'rxjs'
import { filter, map, share, switchMap } from 'rxjs/operators'
import { PatchDB } from 'patch-db-client'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { ConnectionService } from 'src/app/services/connection.service'
import { OSService } from 'src/app/services/os.service'
import { LocalStorageBootstrap } from './patch-db/local-storage-bootstrap'

@Injectable({
  providedIn: 'root',
})
export class PatchDataService extends Observable<void> {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly os = inject(OSService)
  private readonly bootstrapper = inject(LocalStorageBootstrap)
  private readonly stream$ = inject(ConnectionService).pipe(
    filter(Boolean),
    switchMap(() => this.patch.watch$()),
    map((cache, index) => {
      this.bootstrapper.update(cache)

      if (index === 0) {
        this.os.loadOS().catch(console.error)
      }
    }),
    share(),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
