import { inject, Injectable, InjectionToken } from '@angular/core'
import { Dump, Revision, Update } from 'patch-db-client'
import { BehaviorSubject, EMPTY, Observable, timer } from 'rxjs'
import {
  bufferTime,
  catchError,
  filter,
  startWith,
  switchMap,
} from 'rxjs/operators'
import { ApiService, SubscribeRes } from '../api/api.service'
import { AuthService } from '../auth.service'
import { TunnelData } from './data-model'
import { toObservable } from '@angular/core/rxjs-interop'

export const PATCH_CACHE = new InjectionToken('', {
  factory: () =>
    new BehaviorSubject<Dump<TunnelData>>({
      id: 0,
      value: {} as TunnelData,
    }),
})

@Injectable({
  providedIn: 'root',
})
export class PatchDbSource extends Observable<Update<TunnelData>[]> {
  private readonly api = inject(ApiService)
  private readonly auth = inject(AuthService)

  private readonly stream$ = toObservable(this.auth.authenticated).pipe(
    switchMap(verified =>
      verified
        ? this.api.rpc.db
            .subscribe({})
            .then(response => response as SubscribeRes)
        : EMPTY,
    ),
    switchMap(({ dump, guid }) =>
      this.api.openWebsocket$<Revision>(guid).pipe(
        bufferTime(250),
        filter(revisions => !!revisions.length),
        startWith([dump]),
      ),
    ),
    catchError((_, watch$) => timer(500).pipe(switchMap(() => watch$))),
    startWith([{ id: 0, value: {} as TunnelData }]),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
