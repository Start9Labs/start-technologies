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
    switchMap(() =>
      merge(
        this.patch.watch$().pipe(map(cache => this.bootstrapper.update(cache))),
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
