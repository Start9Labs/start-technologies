import type { T } from '@start9labs/start-core'
import {
  defer,
  distinctUntilChanged,
  map,
  Observable,
  shareReplay,
  switchMap,
  timer,
} from 'rxjs'

export function hostTime$(
  synced$: Observable<boolean>,
  load: () => Promise<T.TimeInfo>,
) {
  return synced$.pipe(
    distinctUntilChanged(),
    switchMap(synced =>
      defer(load).pipe(
        switchMap(({ now, uptime }) => {
          const uptimeSecs = Number(uptime)
          const hostNow = new Date(now).valueOf()
          const receivedAt = performance.now()
          return timer(0, 1000).pipe(
            map(() => {
              const elapsed = performance.now() - receivedAt
              return {
                now: hostNow + elapsed,
                uptime: uptimeSecs + Math.floor(elapsed / 1000),
                synced,
              }
            }),
          )
        }),
      ),
    ),
    shareReplay(1),
  )
}
