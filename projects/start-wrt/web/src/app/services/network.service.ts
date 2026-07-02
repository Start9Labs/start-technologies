import { inject, Injectable } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { fromEvent, merge, Observable, shareReplay } from 'rxjs'
import { distinctUntilChanged, map, startWith } from 'rxjs/operators'

@Injectable({ providedIn: 'root' })
export class NetworkService extends Observable<boolean> {
  private readonly win = inject(WA_WINDOW)
  private readonly stream$ = merge(
    fromEvent(this.win, 'online'),
    fromEvent(this.win, 'offline'),
  ).pipe(
    startWith(null),
    map(() => this.win.navigator.onLine),
    distinctUntilChanged(),
    shareReplay(1),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))
  }
}
