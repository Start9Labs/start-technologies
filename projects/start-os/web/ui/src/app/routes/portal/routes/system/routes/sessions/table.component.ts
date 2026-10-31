import { CommonModule } from '@angular/common'
import {
  Component,
  computed,
  input,
  OnChanges,
  output,
  signal,
} from '@angular/core'
import { FormsModule } from '@angular/forms'
import { TuiButton, TuiIcon, TuiCheckbox } from '@taiga-ui/core'
import { TuiFade, TuiSkeleton } from '@taiga-ui/kit'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { T } from '@start9labs/start-core'
import { PlatformInfoPipe } from './platform-info.pipe'
import { i18nPipe } from '@start9labs/shared'

@Component({
  selector: '[sessions]',
  template: `
    <table
      [appTable]="
        single()
          ? ['Name', 'Platform', 'Last Active']
          : ['Platform', 'Last Active']
      "
      [appTableSelected]="selected().length"
    >
      @if (!single()) {
        <th
          class="g-table-select"
          [attr.colspan]="selected().length ? 3 : null"
          [style.text-indent.rem]="1.75"
        >
          <input
            tuiCheckbox
            size="s"
            type="checkbox"
            [disabled]="!sessions()"
            [ngModel]="all()"
            (ngModelChange)="selected.set(($event && sessions()) || [])"
          />
          @if (selected().length) {
            @let count = selected().length;
            <span class="g-table-group">
              <button
                tuiButton
                size="xs"
                appearance="flat-destructive"
                iconStart="@tui.log-out"
                (click)="terminateSelected.emit()"
              >
                {{ 'Terminate' | i18n }}
              </button>
              <span class="count">{{ count }} {{ 'selected' | i18n }}</span>
            </span>
          } @else {
            {{ 'Name' | i18n }}
          }
        </th>
      }
      @for (session of sessions(); track $index) {
        <tr
          (longtap)="!selected().length && onToggle(session)"
          (click)="
            selected().length &&
              $any($event.target).closest('tui-root._mobile') &&
              onToggle(session)
          "
        >
          <td [style.padding-left.rem]="single() ? null : 2.5">
            @if (!single()) {
              <input
                tuiCheckbox
                size="s"
                type="checkbox"
                [ngModel]="selected().includes(session)"
                (ngModelChange)="onToggle(session)"
              />
            }
            <div tuiFade class="agent">
              {{ session.name || session.userAgent || '-' }}
            </div>
          </td>
          @if (session | platformInfo; as platform) {
            <td class="platform">
              <tui-icon [icon]="platform.icon" />
              {{ platform.name }}
            </td>
          }
          <td class="date">{{ session.lastActive | date: 'medium' }}</td>
        </tr>
      } @empty {
        @if (sessions()) {
          <tr>
            <td colspan="3">{{ 'No sessions' | i18n }}</td>
          </tr>
        } @else {
          @for (item of single() ? [''] : ['', '']; track $index) {
            <tr>
              <td colspan="3">
                <div [tuiSkeleton]="true">{{ 'Loading' | i18n }}</div>
              </td>
            </tr>
          }
        }
      }
    </table>
  `,
  styles: `
    td {
      position: relative;
      width: 25%;

      &[colspan] {
        grid-column: span 2;
      }

      &:first-child {
        width: 50%;
      }
    }

    input {
      position: absolute;
      top: 50%;
      left: 0.75rem;
      transform: translateY(-50%);
    }

    .platform {
      white-space: nowrap;
    }

    :host-context(tui-root._mobile) {
      table:has(:checked) .platform {
        visibility: hidden;
      }

      table:not(:has(:checked)) input {
        visibility: hidden;
      }

      tr {
        grid-template-columns: 2.5rem 1fr;
        user-select: none;
      }

      input {
        left: 0.25rem;
        pointer-events: none;
      }

      td {
        width: 100%;

        &:first-child {
          padding: 0 !important;
        }
      }

      .agent {
        white-space: nowrap;
        display: block;
      }

      .platform {
        font-size: 0;
        grid-area: 1 / 1 / 3 / 1;
        place-content: center;
      }

      .date {
        color: var(--tui-text-secondary);
      }
    }
  `,
  imports: [
    CommonModule,
    FormsModule,
    PlatformInfoPipe,
    TuiButton,
    TuiIcon,
    TuiCheckbox,
    TuiFade,
    TuiSkeleton,
    TableComponent,
    i18nPipe,
  ],
})
export class SessionsTableComponent implements OnChanges {
  readonly sessions = input<readonly SessionWithId[] | null>(null)
  readonly single = input(false)
  readonly terminateSelected = output()

  readonly selected = signal<readonly SessionWithId[]>([])
  readonly all = computed(
    () =>
      !!this.selected()?.length &&
      (this.selected().length === this.sessions()?.length || null),
  )

  ngOnChanges() {
    this.selected.set([])
  }

  onToggle(session: SessionWithId) {
    if (this.selected().includes(session)) {
      this.selected.update(selected => selected.filter(s => s !== session))
    } else {
      this.selected.update(selected => [...selected, session])
    }
  }
}

export interface SessionWithId extends T.Session {
  id: string
  kind: 'pubkey' | 'token'
}
