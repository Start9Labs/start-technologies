import { inject, Injectable, InjectionToken } from '@angular/core'
import { databaseSource } from '@start9labs/shared'
import { Dump, Update } from 'patch-db-client'
import { BehaviorSubject, defer, EMPTY, Observable } from 'rxjs'
import {
  catchError,
  filter,
  skip,
  startWith,
  switchMap,
  take,
} from 'rxjs/operators'
import { StateService } from 'src/app/services/state.service'
import { ApiService } from '../api/embassy-api.service'
import { AuthService } from '../auth.service'
import { DataModel } from './data-model'
import { PatchDbCache } from './patch-db-cache'

export const PATCH_CACHE = new InjectionToken('', {
  factory: () =>
    new BehaviorSubject<Dump<DataModel>>({
      id: 0,
      value: {} as DataModel,
    }),
})

@Injectable({
  providedIn: 'root',
})
export class PatchDbSource extends Observable<Update<DataModel>[]> {
  private readonly api = inject(ApiService)
  private readonly state = inject(StateService)
  private readonly cache = inject(PatchDbCache)
  private readonly live$ = inject(AuthService).isVerified$.pipe(
    switchMap(verified =>
      verified ? databaseSource(this.api.watchDb$()) : EMPTY,
    ),
    catchError((_, original$) => {
      this.state.retrigger(false, 2000)

      return this.state.pipe(
        skip(1),
        filter(current => current === 'running'),
        take(1),
        switchMap(() => original$),
      )
    }),
  )
  private readonly stream$ = defer(() => this.cache.read()).pipe(
    switchMap(cached => this.live$.pipe(startWith([cached]))),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
