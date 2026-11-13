import { TuiButton, TuiCheckbox } from '@taiga-ui/core'
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
import { i18nPipe } from '@start9labs/shared'
import { TuiFade, TuiSkeleton } from '@taiga-ui/kit'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { T } from '@start9labs/start-core'

@Component({
  selector: '[keys]',
  template: `
    <table
      [appTable]="['Created At', 'Algorithm', 'Fingerprint']"
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
          [disabled]="!keys()"
          [ngModel]="all()"
          (ngModelChange)="selected.set(($event && keys()) || [])"
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
              {{ 'Remove' | i18n }}
            </button>
            <span class="count">{{ count }} {{ 'selected' | i18n }}</span>
          </span>
        } @else {
          {{ 'Hostname' | i18n }}
        }
      </th>
      @for (key of keys(); track $index) {
        <tr
          (longtap)="!selected().length && onToggle(key)"
          (click)="
            selected().length &&
              $any($event.target).closest('tui-root._mobile') &&
              onToggle(key)
          "
        >
          <td [style.padding-left.rem]="2.5">
            <input
              tuiCheckbox
              size="s"
              type="checkbox"
              [ngModel]="selected().includes(key)"
              (ngModelChange)="onToggle(key)"
            />
            <div tuiFade class="hostname">{{ key.hostname }}</div>
          </td>
          <td class="date">{{ key.createdAt | date: 'medium' }}</td>
          <td class="algorithm">{{ key.alg }}</td>
          <td class="fingerprint" tuiFade>{{ key.fingerprint }}</td>
        </tr>
      } @empty {
        @if (keys()) {
          <tr>
            <td colspan="5">{{ 'No SSH keys' | i18n }}</td>
          </tr>
        } @else {
          @for (i of ['', '']; track $index) {
            <tr>
              <td colspan="5">
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

      &[colspan] {
        grid-column: span 2;
      }
    }

    .date {
      width: 12rem;
    }

    input {
      position: absolute;
      top: 50%;
      left: 0.75rem;
      transform: translateY(-50%);
    }

    :host-context(tui-root._mobile) {
      table {
        &:has(:checked) tr {
          padding-inline-start: 2rem;
        }

        &:not(:has(:checked)) input {
          visibility: hidden;
        }
      }

      tr {
        grid-template-columns: 1fr 5rem;
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

      .hostname {
        order: 1;
        grid-column: span 2;
        font-weight: bold;
        text-transform: uppercase;
      }

      .fingerprint {
        order: 2;
        grid-column: span 2;
      }

      .date {
        order: 3;
        color: var(--tui-text-secondary);
      }

      .algorithm {
        order: 4;
        text-align: end;
      }
    }
  `,
  imports: [
    CommonModule,
    FormsModule,
    TuiButton,
    TuiCheckbox,
    TuiFade,
    TuiSkeleton,
    TableComponent,
    i18nPipe,
  ],
})
export class SSHTableComponent<
  K extends T.SshKeyResponse,
> implements OnChanges {
  readonly keys = input<readonly K[] | null>(null)
  readonly deleteSelected = output()

  readonly selected = signal<readonly K[]>([])
  readonly all = computed(
    () =>
      !!this.selected()?.length &&
      (this.selected().length === this.keys()?.length || null),
  )

  ngOnChanges() {
    this.selected.set([])
  }

  onToggle(key: K) {
    if (this.selected().includes(key)) {
      this.selected.update(selected => selected.filter(s => s !== key))
    } else {
      this.selected.update(selected => [...selected, key])
    }
  }
}
