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
  TuiHint,
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

/** Dropdown value of the managed-switch choice; a profile name is never empty. */
const TRUNK = ''

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
                [textContent]="item.profileName"
                [ngModel]="item.trunk ? TRUNK : item.profileName"
                (ngModelChange)="onProfileChange(item, $event)"
              >
                <tui-data-list *tuiDropdown>
                  @for (profile of service.profiles(); track profile.vlan_tag) {
                    <button tuiOption [value]="profile.fullname">
                      {{ profile.fullname }}
                    </button>
                  }
                  @if (!item.usb) {
                    <hr />
                    <button tuiOption iconEnd="@tui.network" [value]="TRUNK">
                      {{ 'Managed switch' | i18n }}
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
              @if (item.trunk) {
                <div class="g-secondary">
                  {{ 'VLAN IDs' | i18n }}:
                  @for (
                    profile of service.profiles();
                    track profile.vlan_tag;
                    let last = $last
                  ) {
                    {{ profile.fullname }} {{ profile.vlan_tag
                    }}{{ last ? '' : ',' }}
                  }
                </div>
              }
            }
          </td>
          <td tuiTd>
            @if (item.usb; as usb) {
              <button
                tuiIconButton
                type="button"
                size="xs"
                appearance="icon"
                iconStart="@tui.trash"
                [disabled]="usb.present"
                [tuiHint]="
                  usb.present
                    ? ('Unplug the adapter to remove this port' | i18n)
                    : null
                "
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
    TuiHint,
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

  protected readonly TRUNK = TRUNK

  readonly ethernetTable = input<EthernetPortView[]>([])

  async onRemove(item: EthernetPortView) {
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

  async onProfileChange(item: EthernetPortView, value: string) {
    const trunk = value === TRUNK
    if (trunk === item.trunk && (trunk || value === item.profileName)) return

    item.trunk = trunk
    item.profile = trunk
      ? null
      : (this.service.profiles().find(p => p.fullname === value) ?? null)
    item.profileName = trunk
      ? this.i18n.transform('Managed switch')
      : (item.profile?.fullname ?? 'Admin')
    const items = [...this.ethernetTable()]

    let leaving
    try {
      leaving = await this.service.previewPorts(items)
    } catch {
      this.service.refresh()
      return
    }

    const summary = trunk
      ? this.i18n.transform(
          'Every security profile is sent to this port as a tagged VLAN, and the switch decides which profile each of its own ports belongs to. Anyone who can log in to the switch can reach every profile, so protect its password and finish its VLAN setup before connecting it.',
        )
      : fill(
          this.i18n.transform(
            'Devices on {port} will move to the {profile} profile. Most reconnect within a few seconds. If a device then has no Internet access or cannot reach other devices, which is common behind a switch, unplug and replug its cable, or restart the device or the switch. Otherwise it reconnects on its own within 12 hours.',
          ),
          { port: item.name, profile: item.profileName },
        )
    const kept = leavingProfileMessage(this.i18n, leaving)
    const confirmed = await firstValueFrom(
      this.dialogs.open<boolean>(TUI_CONFIRM, {
        label: this.i18n.transform(
          trunk ? 'Connect a managed switch' : 'Change Security Profile?',
        ),
        data: {
          content: kept ? `${summary} ${kept}` : summary,
          yes: this.i18n.transform(
            trunk ? 'Trust this switch' : 'Change Profile',
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
