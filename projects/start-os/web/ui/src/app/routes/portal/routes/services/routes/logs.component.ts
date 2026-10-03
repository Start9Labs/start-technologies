import { Component, inject } from '@angular/core'
import { getPkgId } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { LogsComponent } from 'src/app/routes/portal/components/logs/logs.component'
import { FollowServerLogsReq } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'

@Component({
  template: '<logs [fetchLogs]="fetch" [followLogs]="follow" [context]="id" />',
  styles: `
    :host {
      min-height: 0;
    }
  `,
  host: { class: 'g-subpage' },
  imports: [LogsComponent],
})
export default class ServiceLogsRoute {
  private readonly api = inject(ApiService)

  readonly id = getPkgId()

  readonly follow = async (params: FollowServerLogsReq) =>
    this.api.followPackageLogs({ id: this.id, ...params })

  readonly fetch = async (params: T.LogsParamsInput) =>
    this.api.getPackageLogs({ id: this.id, ...params })
}
