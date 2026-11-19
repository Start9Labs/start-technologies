import { inject, Injectable } from '@angular/core'
import { combineLatest, Observable, shareReplay } from 'rxjs'
import { distinctUntilChanged, map } from 'rxjs/operators'
import { NetworkService } from 'src/app/services/network.service'
import { PATCH_LIVE } from 'src/app/services/patch-db/patch-db-source'
import { StateService } from 'src/app/services/state.service'

/** True only while the server's database is observed live. */
@Injectable({
  providedIn: 'root',
})
export class ConnectionService extends Observable<boolean> {
  private readonly stream$ = combineLatest([
    inject(NetworkService),
    inject(StateService),
    inject(PATCH_LIVE),
  ]).pipe(
    map(([network, state, live]) => network && !!state && live),
    distinctUntilChanged(),
    shareReplay(1),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
