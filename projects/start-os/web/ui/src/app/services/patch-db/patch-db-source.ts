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
  tap,
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

export const PATCH_LIVE = new InjectionToken('', {
  factory: () => new BehaviorSubject(false),
})

@Injectable({
  providedIn: 'root',
})
export class PatchDbSource extends Observable<Update<DataModel>[]> {
  private readonly api = inject(ApiService)
  private readonly state = inject(StateService)
  private readonly cache = inject(PatchDbCache)
  private readonly isLive = inject(PATCH_LIVE)
  private readonly current = inject(PATCH_CACHE)
  private readonly live$ = inject(AuthService).isVerified$.pipe(
    switchMap(verified => (verified ? this.watchLive$() : EMPTY)),
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

  private cursor() {
    const { epoch } = this.cache
    const { id } = this.current.value
    return epoch && id ? { epoch, id } : null
  }

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }

  private watchLive$() {
    return new Observable<Update<DataModel>[]>(subscriber => {
      const subscription = databaseSource(
        this.api
          .watchDb$(() => this.cursor())
          .pipe(
            tap(update => {
              if ('epoch' in update && update.epoch)
                this.cache.epoch = update.epoch
            }),
          ),
      ).subscribe({
        next: updates => {
          subscriber.next(updates)
          if (!subscriber.closed) this.isLive.next(true)
        },
        error: error => subscriber.error(error),
      })
      return () => {
        subscription.unsubscribe()
        this.isLive.next(false)
      }
    })
  }
}
