import { CommonModule } from '@angular/common'
import { Component, computed, inject, input } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { TuiIcon, TuiLink } from '@taiga-ui/core'
import { TuiAvatar, TuiLineClamp } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { EMPTY, first, switchMap } from 'rxjs'
import { ServerNotification } from 'src/app/services/api/api.types'
import { NotificationService } from 'src/app/services/notification.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

@Component({
  selector: '[notificationItem]',
  template: `
    @if (notificationItem(); as item) {
      <td>
        {{ item.createdAt | date: 'MMM d, y, h:mm a' }}
      </td>
      <td class="title" [style.color]="color()">
        <ng-content />
        <tui-icon [icon]="icon()" />
        {{ item.title }}
      </td>
      <td class="service">
        @if (pkg(); as pkg) {
          @if (pkg.stateInfo.manifest; as manifest) {
            <a
              tuiAvatar
              size="s"
              [routerLink]="'/services/' + manifest.id"
              [title]="manifest.title"
            >
              <img [src]="pkg.icon" [alt]="manifest.title" />
            </a>
          } @else {
            {{ item.packageId || '-' }}
          }
        } @else {
          {{ item.packageId || '-' }}
        }
      </td>
      <td class="content">
        <tui-line-clamp
          style="pointer-events: none"
          [linesLimit]="3"
          [lineHeight]="21"
          [content]="item.message"
          (overflownChange)="overflow = $event"
        />
        @if ([1, 2].includes(item.code)) {
          <button tuiLink (click.stop)="onClick(item)">
            {{
              item?.code === 1
                ? ('View report' | i18n)
                : ('View details' | i18n)
            }}
          </button>
        } @else if (overflow) {
          <button tuiLink (click.stop)="onClick(item)">
            {{ 'View full' | i18n }}
          </button>
        }
      </td>
    }
  `,
  host: {
    '[class._new]': '!notificationItem()?.seen',
  },
  styles: `
    :host {
      position: relative;
    }

    :host._new td {
      font-weight: bold;
      color: var(--tui-text-primary);
    }

    /* the checkbox is projected in here but paints in the date column on desktop,
       so its containing block has to be the row rather than this cell */
    .title {
      position: static;
      width: 13rem;
    }

    .service {
      width: 4.25rem;
      text-align: center;
      grid-column: 2;
      grid-row: 1 / 3;
      place-content: center;
    }

    tui-icon {
      font-size: 1rem;
      vertical-align: sub;
    }

    td {
      color: var(--tui-text-secondary);
      grid-column: 1;

      &:first-child {
        width: 12rem;
        padding-inline-start: 2.5rem;
        white-space: nowrap;
      }

      &:last-child {
        grid-column: 1 / 3;
      }
    }

    :host-context(tui-root._mobile) {
      :host {
        grid-template-columns: 1fr 2rem;
        user-select: none;
        gap: 0.5rem;
      }

      td:first-child {
        grid-row: 3;
        padding: 0;
        font: var(--tui-typography-body-s);
        color: var(--tui-text-secondary);
        margin-block-start: -0.25rem;
      }

      .title {
        grid-row: 1;
        width: auto;
        font-weight: bold;
        font-size: 1.2em;
        display: flex;
        align-items: center;
        gap: 0.375rem;
      }

      .content {
        grid-row: 2;
      }

      .service {
        grid-row: 1;
        width: auto;

        &:not(:has(a)) {
          display: none;
        }
      }

      :host-context(table:has(:checked)) tui-icon {
        display: none;
      }
    }
  `,
  imports: [
    CommonModule,
    RouterLink,
    TuiLineClamp,
    TuiLink,
    TuiIcon,
    i18nPipe,
    TuiAvatar,
  ],
})
export class NotificationItemComponent {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  readonly service = inject(NotificationService)

  readonly notificationItem = input<ServerNotification<number>>()

  readonly color = computed((item = this.notificationItem()) =>
    item ? this.service.getColor(item) : '',
  )

  readonly icon = computed((item = this.notificationItem()) =>
    item ? this.service.getIcon(item) : '',
  )

  readonly pkg = toSignal(
    toObservable(this.notificationItem).pipe(
      switchMap(item =>
        item
          ? this.patch.watch$('packageData', item.packageId || '').pipe(first())
          : EMPTY,
      ),
    ),
  )

  overflow = false

  onClick(item: ServerNotification<number>) {
    this.service.viewModal(item)
    item.seen = true
  }
}
