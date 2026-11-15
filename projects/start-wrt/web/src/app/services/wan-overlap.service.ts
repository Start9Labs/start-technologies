import { inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NavigationEnd, Router } from '@angular/router'
import {
  catchError,
  EMPTY,
  filter,
  from,
  map,
  merge,
  switchMap,
  timer,
} from 'rxjs'
import { ApiService, WanOverlap } from './api/api.service'
import { AuthService } from './auth.service'

@Injectable({ providedIn: 'root' })
export class WanOverlapService {
  private readonly api = inject(ApiService)
  private readonly auth = inject(AuthService)

  /** Security Profiles whose subnet the WAN overlaps. */
  readonly conflicts = toSignal(
    merge(
      inject(Router).events.pipe(filter(e => e instanceof NavigationEnd)),
      timer(0, 30_000),
    ).pipe(
      filter(() => this.auth.authenticated()),
      switchMap(() =>
        from(this.api.profilesWanOverlap({})).pipe(
          map(overlap => this.withSuggestion(overlap)),
          catchError(() => EMPTY),
        ),
      ),
    ),
    { initialValue: [] },
  )

  private withSuggestion({ conflicts, free_octet }: WanOverlap) {
    return conflicts.map(conflict => {
      const [first, second] = conflict.subnet.split('.')
      return {
        ...conflict,
        suggestion:
          free_octet === null ? null : `${first}.${second}.${free_octet}.0/24`,
      }
    })
  }
}
