import { inject, Injectable, InjectionToken } from '@angular/core'
import { databaseSource } from '@start9labs/shared'
import { Dump, Update } from 'patch-db-client'
import { BehaviorSubject, EMPTY, Observable, timer } from 'rxjs'
import { catchError, startWith, switchMap } from 'rxjs/operators'
import { ApiService } from '../api/api.service'
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
      verified ? databaseSource(this.api.watchDb$()) : EMPTY,
    ),
    catchError((_, watch$) => timer(500).pipe(switchMap(() => watch$))),
    startWith([{ id: 0, value: {} as TunnelData }]),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
