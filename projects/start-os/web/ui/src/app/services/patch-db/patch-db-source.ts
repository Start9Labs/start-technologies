import { inject, Injectable, InjectionToken } from '@angular/core'
import { databaseSource } from '@start9labs/shared'
import { Dump, Update } from 'patch-db-client'
import { BehaviorSubject, EMPTY, Observable } from 'rxjs'
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
import { LocalStorageBootstrap } from './local-storage-bootstrap'

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
  private readonly stream$ = inject(AuthService).isVerified$.pipe(
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
    startWith([inject(LocalStorageBootstrap).init()]),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
