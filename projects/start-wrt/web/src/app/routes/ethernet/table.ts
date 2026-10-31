import { Component, inject, input } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiTable, TuiTableDirective } from '@taiga-ui/addon-table'
import {
  TuiAppearance,
  TuiDataList,
  TuiDropdown,
  TuiLink,
} from '@taiga-ui/core'
import {
  TUI_CONFIRM,
  TuiBadge,
  TuiButtonSelect,
  TuiChevron,
} from '@taiga-ui/kit'
import { firstValueFrom } from 'rxjs'

import { Placeholder } from 'src/app/components/placeholder'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { fill } from 'src/app/i18n/validation-errors'
import { publishedPortDeletionMessage } from 'src/app/services/published-port-deletion'

import { EthernetPortView, EthernetService } from './service'

@Component({
  selector: '[ethernetTable]',
  template: `
    <thead>
      <tr>
        <th tuiTh [sorter]="'name' | tuiSorter">{{ 'Port' | i18n }}</th>
        <th tuiTh [sorter]="'profileName' | tuiSorter">
          {{ 'Security Profile' | i18n }}
        </th>
      </tr>
    </thead>
    <tbody>
      @for (item of ethernetTable() | tuiTableSort; track $index) {
        <tr>
          <td tuiTd>
            {{ item.name }}
            @if (item.wan) {
              <span tuiBadge size="s" appearance="primary">WAN</span>
            }
          </td>
          <td tuiTd>
            @if (item.wan) {
              -
            } @else {
              <button
                tuiLink
                tuiChevron
                tuiButtonSelect
                tuiDropdownAlign="start"
                [textContent]="item.profile?.fullname || 'Admin'"
                [ngModel]="item.profile?.fullname || 'Admin'"
                (ngModelChange)="onProfileChange(item, $event)"
              >
                <tui-data-list *tuiDropdown>
                  @for (profile of service.profiles(); track profile.vlan_tag) {
                    <button tuiOption [value]="profile.fullname">
                      {{ profile.fullname }}
                    </button>
                  }
                  <hr />
                  <a
                    tuiOption
                    tuiAppearance="action"
                    iconEnd="@tui.user-lock"
                    routerLink="/profiles"
                  >
                    {{ 'Manage Profiles' | i18n }}
                  </a>
                </tui-data-list>
              </button>
            }
          </td>
        </tr>
      } @empty {
        <tr>
          <td tuiTd colspan="2">
            <app-placeholder icon="@tui.inbox">
              {{ 'No ports detected' | i18n }}
            </app-placeholder>
          </td>
        </tr>
      }
    </tbody>
  `,
  styles: `
    :host {
      [tuiLink]::after {
        vertical-align: text-bottom;
      }

      [tuiBadge] {
        vertical-align: baseline;
        margin: 0 0.125rem;
      }
    }
  `,
  hostDirectives: [TuiTableDirective],
  host: { class: 'g-table' },
  imports: [
    TuiTable,
    TuiLink,
    TuiButtonSelect,
    FormsModule,
    TuiDataList,
    TuiDropdown,
    TuiChevron,
    TuiBadge,
    RouterLink,
    TuiAppearance,
    Placeholder,
    i18nPipe,
  ],
})
export class EthernetTable {
  protected readonly service = inject(EthernetService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)

  readonly ethernetTable = input<EthernetPortView[]>([])

  async onProfileChange(item: EthernetPortView, fullname: string) {
    const previousName = item.profile?.fullname ?? 'Admin'
    if (fullname === previousName) return

    const profile =
      this.service.profiles().find(p => p.fullname === fullname) ?? null
    item.profile = profile
    const items = [...this.ethernetTable()]

    let pending
    try {
      pending = await this.service.previewPorts(items)
    } catch {
      this.service.refresh()
      return
    }

    const move = fill(
      this.i18n.transform(
        'Devices on {port} will move to the {profile} profile. Most reconnect within a few seconds. If a device then has no Internet access or cannot reach other devices, which is common behind a switch, unplug and replug its cable, or restart the device or the switch. Otherwise it reconnects on its own within 12 hours.',
      ),
      { port: item.name, profile: fullname },
    )
    const deletion = publishedPortDeletionMessage(
      this.i18n.transform(
        'This change breaks the following published port(s), so they will be deleted: {list}.',
      ),
      pending,
    )
    const confirmed = await firstValueFrom(
      this.dialogs.open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform('Change Security Profile?'),
        data: {
          content: deletion ? `${move} ${deletion}` : move,
          yes: this.i18n.transform(
            pending.length ? 'Delete & Continue' : 'Change Profile',
          ),
          no: this.i18n.transform('Cancel'),
        },
      }),
    ).catch(() => false)

    if (!confirmed) {
      this.service.refresh()
      return
    }

    this.service.save(items)
  }
}
