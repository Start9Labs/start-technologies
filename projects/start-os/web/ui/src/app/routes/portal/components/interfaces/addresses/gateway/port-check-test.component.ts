import { Component, computed, input, output } from '@angular/core'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'
import { TuiButtonLoading } from '@taiga-ui/kit'
import { PortCheckIconComponent } from 'src/app/routes/portal/components/port-check-icon.component'
import { PortCheckWarningsComponent } from 'src/app/routes/portal/components/port-check-warnings.component'
import { TableComponent } from 'src/app/routes/portal/components/table.component'

// One labelled value in the reachability-test row: a column in the desktop
// table and a field in the mobile card.
export type PortCheckField = { label: i18nKey; value: string }

// The shared external-reachability test row used by every Address Requirements
// modal — the IPv4 port forward and the IPv6 firewall in the domain modal, and
// the standalone port forward for a public IP. It renders the desktop table /
// mobile card, the status icon, the Test button, and the hairpinning warning;
// callers supply the labelled values and handle `(test)`.
@Component({
  selector: 'port-check-test',
  template: `
    <table [appTable]="headers()">
      <tr>
        <td>
          @if (testable()) {
            <port-check-icon [result]="result()" [loading]="loading()" />
          }
        </td>
        @for (field of fields(); track $index) {
          <td [attr.data-label]="field.label | i18n">{{ field.value }}</td>
        }
        <td>
          @if (testable()) {
            <button
              tuiButton
              size="s"
              [loading]="loading()"
              [disabled]="disabled()"
              (click)="test.emit()"
            >
              {{ 'Test' | i18n }}
            </button>
          }
        </td>
      </tr>
    </table>

    <port-check-warnings [result]="result()" [local]="local()" />
  `,
  styles: `
    table {
      margin-block-end: 2rem;
    }

    tr {
      grid-template-columns: min-content 1fr min-content;
      margin-inline: 1rem;
    }

    td:first-child {
      inline-size: 0;
      min-inline-size: fit-content;
      place-self: center;
      grid-column: 1;
      grid-row: 1 / span 100;
      margin-inline-end: 1rem;

      &:empty {
        display: none;
      }
    }

    td:last-child {
      text-align: end;
      padding-inline: 0.5rem;
      grid-column: 3;
      grid-row: 1 / span 100;
      place-self: center;
    }

    :host-context(tui-root._mobile) {
      table {
        color: var(--tui-text-primary);
        border-radius: var(--tui-radius-l);
        box-shadow: inset 0 0 0 1px var(--tui-border-normal);
      }

      td[data-label] {
        grid-column: 2;

        &::before {
          content: attr(data-label) ': ';
          color: var(--tui-text-secondary);
        }
      }
    }
  `,
  imports: [
    TuiButton,
    TuiButtonLoading,
    i18nPipe,
    TableComponent,
    PortCheckIconComponent,
    PortCheckWarningsComponent,
  ],
})
export class PortCheckTestComponent {
  // The labelled values to display (external/internal port, or an address).
  readonly fields = input.required<readonly PortCheckField[]>()
  // A port range is display-only — no reachability probe, hence no status icon
  // or Test button; its columns keep their range headers.
  readonly testable = input(true)
  // The IPv4 port-forward check, or the IPv6 firewall sub-result.
  readonly result = input<T.CheckPortRes | T.CheckPortV6Res>()
  readonly loading = input(false)
  readonly disabled = input(false)
  readonly local = input(true)
  readonly test = output<void>()

  // A testable row brackets its value columns with a status icon and a Test
  // button; a range row shows only the columns.
  readonly headers = computed<Array<i18nKey | null>>(() => {
    const labels = this.fields().map(f => f.label)
    return this.testable() ? [null, ...labels, null] : [...labels]
  })
}
