import { inject, Injectable } from '@angular/core'
import { defer, EMPTY, merge, Observable } from 'rxjs'
import {
  filter,
  finalize,
  ignoreElements,
  skip,
  switchMap,
  take,
  tap,
} from 'rxjs/operators'
import { AuthService } from 'src/app/services/auth.service'
import { StateService } from 'src/app/services/state.service'
import { PatchDbCache } from './patch-db/patch-db-cache'
import {
  ALWAYS_WATCHED,
  PATCH_DB,
  PATCH_LIVE,
} from './patch-db/patch-db-source'

/** Runs the database session while verified: restore, subscribe, recover. */
@Injectable({
  providedIn: 'root',
})
export class PatchMonitorService extends Observable<unknown> {
  private readonly patch = inject(PATCH_DB)
  private readonly cache = inject(PatchDbCache)
  private readonly state = inject(StateService)
  private readonly isLive = inject(PATCH_LIVE)

  private readonly session$ = defer(() => this.cache.read()).pipe(
    switchMap(({ dump, cursors }) => {
      this.patch.restore(dump, cursors)
      return merge(
        ...ALWAYS_WATCHED.map(path =>
          this.patch.watch$(...path).pipe(ignoreElements()),
        ),
        this.patch
          .live$(...ALWAYS_WATCHED)
          .pipe(tap(live => this.isLive.next(live))),
        this.patch.failures$.pipe(
          switchMap(() => {
            this.state.retrigger(false, 2000)
            return this.state.pipe(
              skip(1),
              filter(current => current === 'running'),
              take(1),
              tap(() => this.patch.start()),
            )
          }),
        ),
        defer(() => {
          this.patch.start()
          return EMPTY
        }),
      )
    }),
    finalize(() => {
      this.isLive.next(false)
      this.patch.stop()
    }),
  )

  private readonly stream$ = inject(AuthService).isVerified$.pipe(
    switchMap(verified => (verified ? this.session$ : EMPTY)),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
