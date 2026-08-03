import {
  Component,
  computed,
  input,
  OnChanges,
  output,
  signal,
} from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiCheckbox } from '@taiga-ui/core'
import { TuiSkeleton } from '@taiga-ui/kit'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { ServerNotification } from 'src/app/services/api/api.types'
import { PlaceholderComponent } from '../../components/placeholder.component'
import { NotificationItemComponent } from './item.component'

@Component({
  selector: '[notifications]',
  template: `
    <table
      [appTable]="['Title', 'Service', 'Message']"
      [appTableSelected]="selected().length"
    >
      <th
        class="g-table-select"
        [attr.colspan]="selected().length ? 4 : null"
        [style.text-indent.rem]="1.75"
      >
        <input
          tuiCheckbox
          size="s"
          type="checkbox"
          [disabled]="!notifications()"
          [ngModel]="all()"
          (ngModelChange)="selected.set(($event && notifications()) || [])"
        />
        @if (selected().length) {
          @let count = selected().length;
          <span class="g-table-group">
            <button
              tuiButton
              size="xs"
              appearance="flat-destructive"
              iconStart="@tui.trash"
              (click)="deleteSelected.emit()"
            >
              {{ 'Delete' | i18n }}
            </button>
            <span class="count">{{ count }} {{ 'selected' | i18n }}</span>
          </span>
        } @else {
          {{ 'Date' | i18n }}
        }
      </th>
      @for (not of notifications(); track not) {
        <tr
          [notificationItem]="not"
          (longtap)="!selected().length && onToggle(not)"
          (click)="
            selected().length &&
              $any($event.target).closest('tui-root._mobile') &&
              onToggle(not)
          "
        >
          <input
            tuiCheckbox
            size="s"
            type="checkbox"
            [ngModel]="selected().includes(not)"
            (ngModelChange)="onToggle(not)"
          />
        </tr>
      } @empty {
        @if (notifications()) {
          <app-placeholder icon="@tui.bell">
            {{ 'No notifications' | i18n }}
          </app-placeholder>
        } @else {
          @for (i of ['', '', '', '']; track $index) {
            <tr>
              <td colspan="4">
                <div [tuiSkeleton]="true">{{ 'Loading' | i18n }}</div>
              </td>
            </tr>
          }
        }
      }
    </table>
  `,
  styles: `
    :host-context(tui-root:not(._mobile)) input {
      position: absolute;
      top: 50%;
      left: 0.75rem;
      transform: translateY(-50%);
    }

    :host-context(tui-root._mobile) {
      input {
        pointer-events: none;
      }

      :host:not(:has(:checked)) input {
        display: none;
      }
    }
  `,
  imports: [
    FormsModule,
    TuiButton,
    TuiCheckbox,
    NotificationItemComponent,
    TuiSkeleton,
    i18nPipe,
    TableComponent,
    PlaceholderComponent,
  ],
})
export class NotificationsTableComponent<
  T extends ServerNotification<number>,
> implements OnChanges {
  readonly notifications = input<readonly T[] | null>(null)
  readonly deleteSelected = output()

  readonly selected = signal<readonly T[]>([])
  readonly all = computed(
    () =>
      !!this.selected()?.length &&
      (this.selected().length === this.notifications()?.length || null),
  )

  ngOnChanges() {
    this.selected.set([])
  }

  onToggle(notification: T) {
    if (this.selected().includes(notification)) {
      this.selected.update(selected => selected.filter(s => s !== notification))
    } else {
      this.selected.update(selected => [...selected, notification])
    }
  }
}
