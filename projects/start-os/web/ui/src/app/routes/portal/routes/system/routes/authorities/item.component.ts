import { Component, inject, input } from '@angular/core'
import { i18nPipe, ROOT_CA_DOWNLOAD_HREF } from '@start9labs/shared'
import { TuiButton, TuiDataList, TuiDropdown, TuiInput } from '@taiga-ui/core'
import { Authority, AuthorityService } from './authority.service'

@Component({
  selector: 'tr[authority]',
  template: `
    @if (authority(); as authority) {
      <td>{{ authority.name }}</td>
      <td>{{ authority.url || '-' }}</td>
      <td class="hidden">{{ authority.contact?.join(', ') || '-' }}</td>
      <td>
        <button
          tuiIconButton
          tuiDropdown
          tuiDropdownAlign="end"
          size="s"
          appearance="flat-grayscale"
          iconStart="@tui.ellipsis-vertical"
          [tuiAppearanceState]="open ? 'hover' : null"
          [(tuiDropdownOpen)]="open"
        >
          {{ 'More' | i18n }}
          <tui-data-list *tuiDropdown>
            @if (authority.url) {
              <button
                tuiOption
                iconStart="@tui.pencil"
                (click)="service.edit($any(authority))"
              >
                {{ 'Edit' | i18n }}
              </button>
              <hr />
              <button
                tuiOption
                iconStart="@tui.trash"
                class="g-negative"
                (click)="service.remove($any(authority))"
              >
                {{ 'Delete' | i18n }}
              </button>
            } @else {
              <a
                tuiOption
                download
                iconStart="@tui.download"
                [href]="rootCaHref"
              >
                {{ 'Download' | i18n }}
              </a>
            }
          </tui-data-list>
        </button>
      </td>
    }
  `,
  styles: `
    td:last-child {
      grid-area: 1 / 2 / 4;
      align-self: center;
      text-align: right;
    }

    :host-context(tui-root._mobile) {
      grid-template-columns: 1fr min-content;

      .hidden {
        display: none;
      }
    }
  `,
  imports: [TuiButton, i18nPipe, TuiDropdown, TuiDataList, TuiInput],
})
export class AuthorityItemComponent {
  protected readonly service = inject(AuthorityService)

  readonly authority = input.required<Authority>()
  readonly rootCaHref = ROOT_CA_DOWNLOAD_HREF

  open = false
}
