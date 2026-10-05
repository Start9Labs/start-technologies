import { inject, Injectable, InjectionToken } from '@angular/core'
import { Dump, Revision, Update } from 'patch-db-client'
import { BehaviorSubject, EMPTY, firstValueFrom, Observable } from 'rxjs'
import {
  bufferTime,
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

export async function waitForPatchSequence(
  cache$: Observable<Dump<DataModel>>,
  sequence: number,
): Promise<void> {
  await firstValueFrom(cache$.pipe(filter(({ id }) => id >= sequence)))
}

@Injectable({
  providedIn: 'root',
})
export class PatchDbSource extends Observable<Update<DataModel>[]> {
  private readonly api = inject(ApiService)
  private readonly state = inject(StateService)
  private readonly stream$ = inject(AuthService).isVerified$.pipe(
    switchMap(verified => (verified ? this.api.subscribeToPatchDB({}) : EMPTY)),
    switchMap(({ dump, guid }) =>
      this.api.openWebsocket$<Revision>(guid).pipe(
        bufferTime(250),
        filter(revisions => !!revisions.length),
        startWith([dump]),
      ),
    ),
    catchError((_, original$) => {
      this.state.retrigger(false, 2000)

      return this.state.pipe(
        skip(1), // skipping previous value stored due to shareReplay
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
