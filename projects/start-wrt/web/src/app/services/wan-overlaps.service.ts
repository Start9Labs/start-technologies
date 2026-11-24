import { inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NavigationEnd, Router } from '@angular/router'
import { catchError, EMPTY, filter, from, merge, switchMap, timer } from 'rxjs'
import { ApiService, WanOverlap } from './api/api.service'
import { AuthService } from './auth.service'

@Injectable({ providedIn: 'root' })
export class WanOverlapsService {
  private readonly api = inject(ApiService)
  private readonly auth = inject(AuthService)

  readonly overlaps = toSignal(
    merge(
      inject(Router).events.pipe(filter(e => e instanceof NavigationEnd)),
      timer(0, 30_000),
    ).pipe(
      filter(() => this.auth.authenticated()),
      switchMap(() =>
        from(this.api.profilesWanOverlaps()).pipe(catchError(() => EMPTY)),
      ),
    ),
    { initialValue: [] as WanOverlap[] },
  )
}
