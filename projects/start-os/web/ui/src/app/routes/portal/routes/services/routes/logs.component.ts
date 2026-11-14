import { Component, inject } from '@angular/core'
import { getPkgId } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { LogsComponent } from 'src/app/routes/portal/components/logs/logs.component'
import { FollowLogs } from 'src/app/services/api/api.types'
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

  readonly follow: FollowLogs = (params, options) =>
    this.api.streams.package.logs.follow({ id: this.id, ...params }, options)

  readonly fetch = async (params: T.LogsParamsInput) =>
    this.api.rpc.package.logs({ id: this.id, ...params })
}
