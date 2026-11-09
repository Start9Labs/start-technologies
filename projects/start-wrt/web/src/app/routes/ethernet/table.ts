import { Component, inject, input } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiTable, TuiTableDirective } from '@taiga-ui/addon-table'
import {
  TuiAppearance,
  TuiButton,
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
import { leavingProfileMessage } from 'src/app/services/leaving-profile'

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
        <th tuiTh></th>
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
            @if (item.usb?.active === false) {
              <span tuiBadge size="s" appearance="neutral">
                {{ 'Not connected' | i18n }}
              </span>
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
          <td tuiTd>
            @if (item.usb) {
              <button
                tuiIconButton
                type="button"
                size="xs"
                appearance="icon"
                iconStart="@tui.trash"
                (click)="onRemove(item)"
              >
                {{ 'Remove' | i18n }}
              </button>
            }
          </td>
        </tr>
      } @empty {
        <tr>
          <td tuiTd colspan="3">
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

      [tuiTd]:last-child {
        padding-block: 0;
        text-align: end;
      }
    }
  `,
  hostDirectives: [TuiTableDirective],
  host: { class: 'g-table' },
  imports: [
    TuiTable,
    TuiButton,
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

  async onRemove(item: EthernetPortView) {
    if (await this.service.usbPortInUse(item.name)) {
      this.dialogs
        .open(
          fill(
            this.i18n.transform(
              'Unplug the adapter from {port}, then remove the port.',
            ),
            { port: item.name },
          ),
          {
            label: this.i18n.transform('Adapter plugged in'),
            data: this.i18n.transform('Got it'),
          },
        )
        .subscribe()
      return
    }

    const confirmed = await firstValueFrom(
      this.dialogs.open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform('Remove USB port?'),
        data: {
          content: fill(
            this.i18n.transform(
              'Removing {port} briefly restarts the network. The port appears again, in the Admin profile, the next time a USB Ethernet adapter is plugged into it.',
            ),
            { port: item.name },
          ),
          yes: this.i18n.transform('Remove'),
          no: this.i18n.transform('Cancel'),
        },
      }),
    ).catch(() => false)

    if (confirmed) this.service.removeUsbPort(item.name)
  }

  async onProfileChange(item: EthernetPortView, fullname: string) {
    const previousName = item.profile?.fullname ?? 'Admin'
    if (fullname === previousName) return

    const profile =
      this.service.profiles().find(p => p.fullname === fullname) ?? null
    item.profile = profile
    const items = [...this.ethernetTable()]

    let leaving
    try {
      leaving = await this.service.previewPorts(items)
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
    const kept = leavingProfileMessage(this.i18n, leaving)
    const confirmed = await firstValueFrom(
      this.dialogs.open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform('Change Security Profile?'),
        data: {
          content: kept ? `${move} ${kept}` : move,
          yes: this.i18n.transform('Change Profile'),
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
