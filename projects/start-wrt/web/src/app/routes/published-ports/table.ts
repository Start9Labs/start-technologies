import { Component, inject, input, output } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiTable, TuiTableDirective } from '@taiga-ui/addon-table'
import { TuiComparator } from '@taiga-ui/addon-table/types'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiHint,
  TuiLink,
} from '@taiga-ui/core'
import { Copy } from 'src/app/components/copy'
import { Placeholder } from 'src/app/components/placeholder'
import { PublishedPortDisplay } from './types'
import { PublishedPortsService } from './service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import {
  confirmVpnExposedPort,
  VpnExposure,
} from 'src/app/services/vpn-exposed-port'

const PROTOCOL_LABELS = {
  tcp: 'TCP',
  udp: 'UDP',
  'tcp+udp': 'TCP + UDP',
} as const

@Component({
  selector: '[publishedPorts]',
  template: `
    <thead tuiThead>
      <tr>
        <th tuiTh [sorter]="'status' | tuiSorter" [style.width.rem]="3"></th>
        <th tuiTh [sorter]="'label' | tuiSorter" [style.min-width.rem]="10.5">
          {{ 'Label' | i18n }}
        </th>
        <th
          tuiTh
          [sorter]="'deviceName' | tuiSorter"
          [style.min-width.rem]="10"
        >
          {{ 'Device' | i18n }}
        </th>
        <th tuiTh [sorter]="'ports' | tuiSorter" [style.min-width.rem]="6">
          {{ 'Port' | i18n }}
        </th>
        <th tuiTh [sorter]="'protocol' | tuiSorter" [style.min-width.rem]="8">
          {{ 'Protocol' | i18n }}
        </th>
        <th tuiTh [sorter]="ipSorter" [style.min-width.rem]="9">
          {{ 'IP Version' | i18n }}
        </th>
        <th tuiTh [sorter]="'source' | tuiSorter" [style.min-width.rem]="6">
          {{ 'Source' | i18n }}
        </th>
        <th tuiTh [style.min-width.rem]="16">{{ 'Endpoints' | i18n }}</th>
        <th tuiTh [style.width.rem]="3"></th>
      </tr>
    </thead>
    <tbody>
      @for (item of publishedPorts() | tuiTableSort; track item.id) {
        <tr>
          <td tuiTd>
            <span
              [style.cursor]="'help'"
              [tuiHint]="statusHint"
              tuiHintDirection="end"
            >
              {{ statusIcon(item) }}
            </span>
            <ng-template #statusHint>
              <span class="flex">
                <strong>{{ item.status | i18n }}</strong>
                @if (item.statusReason) {
                  <span>{{ item.statusReason }}</span>
                }
              </span>
            </ng-template>
          </td>
          <td tuiTd>
            <strong>{{ item.label }}</strong>
          </td>
          <td tuiTd>
            <a
              tuiLink
              routerLink="/devices/device"
              [queryParams]="{ mac: item.deviceMac }"
              [state]="{ returnUrl: '/published-ports' }"
            >
              {{ item.deviceName }}
            </a>
          </td>
          <td tuiTd>{{ item.ports }}</td>
          <td tuiTd>{{ protocolLabel(item.protocol) | i18n }}</td>
          <td tuiTd>{{ ipVersionLabel(item) | i18n }}</td>
          <td tuiTd>
            {{ item.source === 'any' ? ('Any' | i18n) : item.source }}
          </td>
          <td tuiTd>
            <div class="flex">
              @if (getEndpointIpv4(item); as endpoint) {
                <button tuiLink appCopy>{{ endpoint }}</button>
              }
              @if (getEndpointIpv6(item); as endpoint) {
                <button tuiLink appCopy>{{ endpoint }}</button>
              }
              @if (!getEndpointIpv4(item) && !getEndpointIpv6(item)) {
                <span class="g-secondary">—</span>
              }
            </div>
          </td>
          <td tuiTd>
            <button
              tuiIconButton
              size="xs"
              iconStart="@tui.ellipsis-vertical"
              appearance="icon"
              tuiDropdownAlign="end"
              tuiDropdownAuto
              tuiDropdown
            >
              {{ 'Actions' | i18n }}
              <tui-data-list
                *tuiDropdown="let close"
                size="s"
                (click)="close()"
              >
                <button
                  tuiOption
                  [iconStart]="
                    item.enabled ? '@tui.circle-pause' : '@tui.circle-play'
                  "
                  (click)="toggleEnabled(item)"
                >
                  {{ item.enabled ? ('Disable' | i18n) : ('Enable' | i18n) }}
                </button>
                <button
                  tuiOption
                  iconStart="@tui.pencil"
                  (click)="edit.emit(item)"
                >
                  {{ 'Edit' | i18n }}
                </button>
                <button
                  tuiOption
                  class="g-negative"
                  iconStart="@tui.trash"
                  (click)="delete(item.id)"
                >
                  {{ 'Delete' | i18n }}
                </button>
              </tui-data-list>
            </button>
          </td>
        </tr>
      } @empty {
        <tr>
          <td tuiTd colspan="9">
            <app-placeholder icon="@tui.globe">
              {{ 'No ports have been published' | i18n }}
            </app-placeholder>
          </td>
        </tr>
      }
    </tbody>
  `,
  styles: `
    td:first-child {
      text-align: center;
    }

    .flex {
      display: flex;
      flex-direction: column;
      align-items: flex-start;
      gap: 0.25rem;

      strong::first-letter {
        text-transform: uppercase;
      }
    }
  `,
  hostDirectives: [TuiTableDirective],
  host: { class: 'g-table' },
  imports: [
    RouterLink,
    TuiTable,
    TuiButton,
    TuiDataList,
    TuiDropdown,
    TuiHint,
    TuiLink,
    Placeholder,
    Copy,
    i18nPipe,
  ],
})
export class PublishedPortsTable {
  private readonly service = inject(PublishedPortsService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)

  public readonly publishedPorts = input<PublishedPortDisplay[]>([])
  public readonly ipv4EndpointHost = input<string | null>(null)
  public readonly ipv6Available = input(true)
  // Profiles whose outbound is a VPN, keyed by lower-cased fullname → VPN label,
  // plus the default (LAN-owning) profile's fullname. Used to warn when a port
  // is re-enabled for a device whose profile routes through a VPN (the port
  // stays exposed on the public WAN IP).
  public readonly vpnProfiles = input<Map<string, string>>(new Map())
  public readonly defaultProfile = input('')
  public readonly edit = output<PublishedPortDisplay>()

  protected readonly ipSorter: TuiComparator<PublishedPortDisplay> = (a, b) =>
    getIpVersion(a).localeCompare(getIpVersion(b))

  protected statusIcon(item: PublishedPortDisplay) {
    switch (item.status) {
      case 'active':
        return '🟢'
      case 'disabled':
        return '⚪'
      case 'error':
        return '🔴'
      case 'partial':
        return '🟡'
      case 'paused':
        return '🟠'
    }
  }

  protected protocolLabel(protocol: string) {
    return PROTOCOL_LABELS[protocol as keyof typeof PROTOCOL_LABELS] || protocol
  }

  protected ipVersionLabel(item: PublishedPortDisplay): string {
    const version = getIpVersion(item)

    if (version === 'both') return 'IPv4 + IPv6'

    return version === 'ipv6' ? 'IPv6' : 'IPv4'
  }

  protected getEndpointIpv4(item: PublishedPortDisplay): string | null {
    if (!item.ipv4) return null

    const host = this.ipv4EndpointHost()
    const port = item.ipv4PublicPort || item.ports

    return host ? `${host}:${port}` : null
  }

  protected getEndpointIpv6(item: PublishedPortDisplay): string | null {
    if (!item.ipv6 || !item.deviceIpv6) return null
    // Don't show non-routable addresses:
    // - fe80:: = link-local (not routable)
    // - fd = ULA (local only, requires WAN IPv6 for global address)
    // - :: = incomplete/placeholder
    return item.deviceIpv6.startsWith('fe80:') ||
      item.deviceIpv6.startsWith('fd') ||
      item.deviceIpv6.startsWith('::')
      ? null
      : `[${item.deviceIpv6}]:${item.ports}`
  }

  // Resolve the VPN exposure for a port's device, or null when its profile
  // doesn't route through a VPN.
  private vpnExposureFor(item: PublishedPortDisplay): VpnExposure | null {
    const device = this.service.getDevice(item.deviceMac)
    const profile = device?.securityProfile || this.defaultProfile()
    const vpn = this.vpnProfiles().get(profile.toLowerCase())
    return vpn ? { profile, vpn, labels: [item.label] } : null
  }

  protected async toggleEnabled(item: PublishedPortDisplay) {
    const enabling = !item.enabled

    // Re-enabling exposes the port again: if the device's profile now routes
    // through a VPN, confirm before exposing it on the public WAN IP.
    if (
      enabling &&
      !(await confirmVpnExposedPort(
        this.dialogs,
        this.i18n,
        this.vpnExposureFor(item),
      ))
    ) {
      return
    }

    // Re-read the list AFTER the await: FormService auto-refreshes every 5s, so
    // the dialog may have outlived the objects we captured. Locate the port by
    // id and rebuild immutably — mutating the stale `item` and spreading the
    // live array would silently drop the toggle.
    const list = this.publishedPorts()
    const current = list.find(p => p.id === item.id)
    if (!current) return

    // Clear IPv6 if no longer available
    const ipv6 =
      enabling && current.ipv6 && !this.ipv6Available() ? false : current.ipv6
    const updated: PublishedPortDisplay = {
      ...current,
      enabled: enabling,
      ipv6,
      status: enabling ? 'active' : 'disabled',
    }

    if (enabling && (updated.ipv4 || updated.ipv6)) {
      this.service.reserveDeviceIps(
        updated.deviceMac,
        updated.ipv4,
        updated.ipv6,
      )
    }

    this.service.save(list.map(p => (p.id === item.id ? updated : p)))
  }

  protected delete(id: string) {
    this.service.save(this.publishedPorts().filter(item => item.id !== id))
  }
}

function getIpVersion(item: PublishedPortDisplay): 'ipv4' | 'ipv6' | 'both' {
  if (item.ipv4 && item.ipv6) return 'both'

  return item.ipv6 ? 'ipv6' : 'ipv4'
}
