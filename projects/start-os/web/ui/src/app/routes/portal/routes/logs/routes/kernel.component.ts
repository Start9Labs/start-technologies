import { Component, inject } from '@angular/core'
import { RouterLink } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'
import { LogsComponent } from 'src/app/routes/portal/components/logs/logs.component'
import { FollowServerLogsReq } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { TitleDirective } from 'src/app/services/title.service'

@Component({
  template: `
    <ng-container *title>
      <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
        {{ 'Back' | i18n }}
      </a>
      {{ 'Kernel Logs' | i18n }}
    </ng-container>
    <logs context="kernel" [followLogs]="follow" [fetchLogs]="fetch" />
  `,
  styles: `
    :host {
      min-height: 0;
    }
  `,
  host: { class: 'g-subpage' },
  imports: [LogsComponent, TitleDirective, RouterLink, TuiButton, i18nPipe],
})
export default class SystemKernelComponent {
  private readonly api = inject(ApiService)

  protected readonly follow = (params: FollowServerLogsReq) =>
    this.api.followKernelLogs(params)

  protected readonly fetch = (params: T.LogsParamsInput) =>
    this.api.getKernelLogs(params)
}
