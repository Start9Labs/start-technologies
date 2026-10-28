import { inject, Injectable } from '@angular/core'
import {
  DialogService,
  ErrorService,
  i18nKey,
  MARKDOWN,
} from '@start9labs/shared'
import { PatchDB } from 'patch-db-client'
import { merge, of, shareReplay, Subject } from 'rxjs'
import { REPORT } from 'src/app/components/backup-report.component'
import { ServerNotification } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

@Injectable({ providedIn: 'root' })
export class NotificationService {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(DialogService)
  private readonly localUnreadCount$ = new Subject<number>()

  readonly unreadCount$ = merge(
    this.patch.watch$('serverInfo', 'unreadNotificationCount'),
    this.localUnreadCount$,
  ).pipe(shareReplay(1))

  async markSeenAll(latestId: number) {
    this.localUnreadCount$.next(0)

    this.api.rpc.notification
      .markSeenBefore({ before: latestId })
      .catch(e => this.errorService.handleError(e))
  }

  getColor(notification: ServerNotification<number>): string {
    switch (notification.level) {
      case 'info':
        return 'var(--tui-text-secondary)'
      case 'success':
        return 'var(--tui-status-positive)'
      case 'warning':
        return 'var(--tui-status-warning)'
      case 'error':
        return 'var(--tui-status-negative)'
      default:
        return ''
    }
  }

  getIcon(notification: ServerNotification<number>): string {
    switch (notification.level) {
      case 'info':
        return '@tui.info'
      case 'success':
        return '@tui.circle-check'
      case 'warning':
      case 'error':
        return '@tui.circle-alert'
      default:
        return ''
    }
  }

  viewModal(notification: ServerNotification<number>) {
    const { data, createdAt, code, title, message } = notification

    switch (code) {
      // Backup report - structured report with per-service results
      case 1:
        this.dialogs
          .openComponent(REPORT, {
            label: 'Backup Report',
            data: { content: data, createdAt },
            size: 'l',
          })
          .subscribe()
        break

      // OS update - data contains the full release notes markdown
      case 2:
        this.dialogs
          .openComponent(MARKDOWN, {
            label: title as i18nKey,
            data: of(data),
            size: 'l',
          })
          .subscribe()
        break

      // General notification - show the message text
      default:
        this.dialogs
          .openComponent(MARKDOWN, {
            label: title as i18nKey,
            data: of(message),
            size: 'l',
          })
          .subscribe()
        break
    }
  }
}
