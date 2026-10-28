import { inject, Injectable } from '@angular/core'
import { PatchDB } from 'patch-db-client'
import { combineLatest, defer, map, shareReplay, switchMap, timer } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

@Injectable({
  providedIn: 'root',
})
export class TimeService {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly time$ = defer(() =>
    inject(ApiService).rpc.server.time({}),
  ).pipe(
    switchMap(({ now, uptime }) => {
      const uptimeSecs = Number(uptime)
      return timer(0, 1000).pipe(
        map(index => ({
          now: new Date(now).valueOf() + 1000 * index,
          uptime: uptimeSecs + index,
        })),
      )
    }),
    shareReplay(1),
  )

  readonly now$ = combineLatest([
    this.time$,
    this.patch.watch$('serverInfo', 'ntpSynced'),
  ]).pipe(map(([{ now }, synced]) => ({ now, synced })))

  readonly uptime$ = this.time$.pipe(
    map(({ uptime }) => {
      const days = Math.floor(uptime / (24 * 60 * 60))
      const daysSec = uptime % (24 * 60 * 60)
      const hours = Math.floor(daysSec / (60 * 60))
      const hoursSec = uptime % (60 * 60)
      const minutes = Math.floor(hoursSec / 60)
      const seconds = uptime % 60
      return { days, hours, minutes, seconds }
    }),
  )
}
