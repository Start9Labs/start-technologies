import {
  Component,
  inject,
  Injector,
  OnInit,
  signal,
  viewChild,
} from '@angular/core'
import { takeUntilDestroyed } from '@angular/core/rxjs-interop'
import { ActivatedRoute, Router } from '@angular/router'
import {
  DialogService,
  ErrorService,
  i18nPipe,
  isEmptyObject,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { filter } from 'rxjs'
import { distinctUntilChanged, skip } from 'rxjs/operators'
import { ServerNotification } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { BadgeService } from 'src/app/services/badge.service'
import { NotificationService } from 'src/app/services/notification.service'
import { TitleDirective } from 'src/app/services/title.service'
import { NotificationSettings } from './settings'
import { NotificationsTableComponent } from './table.component'

@Component({
  template: `
    <ng-container *title>{{ 'Notifications' | i18n }}</ng-container>
    <header tuiHeader="body-l">
      <aside tuiAccessories [style.margin-inline-start]="'auto'">
        <button
          tuiIconButton
          type="button"
          appearance="icon"
          iconStart="@tui.settings"
          (click)="settings()"
        >
          {{ 'Notification settings' | i18n }}
        </button>
      </aside>
    </header>
    @if (table()?.selected()?.length) {
      <button
        class="fab"
        tuiButton
        appearance="primary-destructive"
        iconStart="@tui.trash"
        (click)="remove(notifications() || [])"
      ></button>
    }
    <div
      [notifications]="notifications()"
      (deleteSelected)="remove(notifications() || [])"
    ></div>
  `,
  styles: `
    :host {
      padding: 1rem;
    }

    .fab {
      position: fixed;
      right: 1rem;
      bottom: 1rem;
      z-index: 1;
    }

    :host-context(tui-root:not(._mobile)) .fab {
      display: none;
    }
  `,
  host: { class: 'g-page' },
  imports: [
    TuiButton,
    NotificationsTableComponent,
    TitleDirective,
    i18nPipe,
    TuiHeader,
  ],
})
export default class NotificationsComponent implements OnInit {
  private readonly router = inject(Router)
  private readonly route = inject(ActivatedRoute)
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(DialogService)
  private readonly injector = inject(Injector)

  readonly service = inject(NotificationService)
  readonly api = inject(ApiService)
  readonly errorService = inject(ErrorService)
  readonly notifications = signal<T.NotificationWithId[] | null>(null)

  protected readonly table = viewChild<
    NotificationsTableComponent<ServerNotification<number>>
  >(NotificationsTableComponent)

  protected readonly badge = inject(BadgeService)
    .getCount('notifications')
    .pipe(
      distinctUntilChanged(),
      filter(Boolean),
      skip(1),
      takeUntilDestroyed(),
    )
    .subscribe(() => this.init())

  ngOnInit() {
    this.route.queryParams.subscribe(params => {
      this.router.navigate([], { relativeTo: this.route, queryParams: {} })

      if (isEmptyObject(params)) {
        this.init()
      }
    })
  }

  protected settings() {
    this.dialogs
      .openComponent(
        new PolymorpheusComponent(NotificationSettings, this.injector),
      )
      .subscribe()
  }

  async getMore(params: T.ListNotificationParams) {
    try {
      this.notifications.set(null)
      this.notifications.set(await this.api.rpc.notification.list(params))
    } catch (e: any) {
      this.errorService.handleError(e)
    }
  }

  async remove(all: T.NotificationWithId[]) {
    const ids =
      this.table()
        ?.selected()
        .map(n => n.id) || []

    this.tasks.run(async () => {
      await this.api.rpc.notification.remove({ ids })
      this.notifications.set(all.filter(n => !ids.includes(n.id)))
    }, 'Deleting')
  }

  private init() {
    this.getMore({ before: null, limit: null }).then(() => {
      const latest = this.notifications()?.at(0)
      if (latest) {
        this.service.markSeenAll(latest.id)
      }
    })
  }
}
