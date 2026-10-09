import { inject, Injectable } from '@angular/core'
import { PatchDB } from 'patch-db-client'
import { map } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { hostTime$ } from './host-time'

@Injectable({
  providedIn: 'root',
})
export class TimeService {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly api = inject(ApiService)
  private readonly time$ = hostTime$(
    this.patch.watch$('serverInfo', 'ntpSynced'),
    () => this.api.getSystemTime({}),
  )

  readonly now$ = this.time$.pipe(map(({ now, synced }) => ({ now, synced })))

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
