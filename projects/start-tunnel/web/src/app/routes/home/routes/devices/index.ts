import { Component, computed, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { ErrorService, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiComparator, TuiTable } from '@taiga-ui/addon-table'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiLoader,
  TuiTitle,
} from '@taiga-ui/core'
import { TUI_CONFIRM, TuiSkeleton, TuiSwitch } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { filter, map } from 'rxjs'
import { PlaceholderComponent } from 'src/app/routes/home/components/placeholder'
import { defaultWanIp, wanOptions } from 'src/app/routes/home/components/wan'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { ApiService } from 'src/app/services/api/api.service'
import { TunnelData } from 'src/app/services/patch-db/data-model'
import { DEVICES_ADD } from './add'
import { DEVICES_CONFIG } from './config'
import { deviceIpv6, MappedDevice } from './utils'

type DeviceRow = { name: string; subnet: { name: string }; ip: string }

@Component({
  template: `
    <div tuiCardLarge="compact" appearance="floating">
      <header tuiHeader="body-l">
        <tui-icon icon="@tui.server" />
        <h3 tuiTitle>{{ 'Servers' | i18n }}</h3>
        <aside tuiAccessories>
          <button tuiButton iconStart="@tui.plus" (click)="onAdd('server')">
            {{ 'Add' | i18n }}
          </button>
        </aside>
      </header>
      <table tuiTable class="g-table" [tuiSkeleton]="!servers()">
        <thead>
          <tr>
            <th tuiTh [sorter]="byName">{{ 'Name' | i18n }}</th>
            <th tuiTh [sorter]="bySubnet">{{ 'Subnet' | i18n }}</th>
            <th tuiTh [sorter]="byIp">{{ 'LAN IPv4' | i18n }}</th>
            <th tuiTh>{{ 'DNS Injection' | i18n }}</th>
            <th tuiTh>{{ 'Auto-publish' | i18n }}</th>
            <th tuiTh>{{ 'WAN IPv4' | i18n }}</th>
            <th tuiTh>{{ 'IPv6' | i18n }}</th>
            <th tuiTh></th>
          </tr>
        </thead>
        <tbody>
          @for (device of servers() | tuiTableSort; track $index) {
            <tr>
              <td tuiTd>{{ device.name }}</td>
              <td tuiTd>{{ device.subnet.name }}</td>
              <td tuiTd>{{ device.ip }}</td>
              <td tuiTd>
                <tui-loader
                  size="xs"
                  [loading]="togglingDns() === device.ip"
                  [overlay]="true"
                >
                  <input
                    tuiSwitch
                    type="checkbox"
                    size="s"
                    [style.display]="'flex'"
                    [showIcons]="false"
                    [ngModel]="device.allowDnsInjection"
                    (ngModelChange)="onDnsInjection(device)"
                  />
                </tui-loader>
              </td>
              <td tuiTd>
                <tui-loader
                  size="xs"
                  [loading]="togglingPf() === device.ip"
                  [overlay]="true"
                >
                  <input
                    tuiSwitch
                    type="checkbox"
                    size="s"
                    [style.display]="'flex'"
                    [showIcons]="false"
                    [ngModel]="device.allowAutoPortForward"
                    (ngModelChange)="onAutoPortForward(device)"
                  />
                </tui-loader>
              </td>
              <td tuiTd>
                @if (device.wanIp) {
                  {{ device.wanIp }}
                } @else if (device.inheritedWan) {
                  {{ 'Subnet default' | i18n }} ({{ device.inheritedWan }})
                } @else {
                  {{ 'Subnet default' | i18n }}
                }
              </td>
              <td tuiTd>{{ device.ipv6 ?? '—' }}</td>
              <td tuiTd [style.padding-inline-end.rem]="0.625">
                <button
                  tuiIconButton
                  size="xs"
                  tuiDropdown
                  tuiDropdownAuto
                  appearance="flat-grayscale"
                  iconStart="@tui.ellipsis-vertical"
                >
                  {{ 'Actions' | i18n }}
                  <tui-data-list
                    *tuiDropdown="let close"
                    size="s"
                    (click)="close()"
                  >
                    <button
                      tuiOption
                      iconStart="@tui.pencil"
                      (click)="onEdit(device)"
                    >
                      {{ 'Edit' | i18n }}
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.settings"
                      (click)="onConfig(device)"
                    >
                      {{ 'View Config' | i18n }}
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.laptop"
                      (click)="onSetKind(device, 'client')"
                    >
                      {{ 'Change to Client' | i18n }}
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.trash"
                      (click)="onDelete(device)"
                    >
                      {{ 'Delete' | i18n }}
                    </button>
                  </tui-data-list>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="8">
                <app-placeholder icon="@tui.laptop">
                  {{ 'No servers' | i18n }}
                </app-placeholder>
              </td>
            </tr>
          }
        </tbody>
      </table>
    </div>

    <div tuiCardLarge="compact" appearance="floating">
      <header tuiHeader="body-l">
        <tui-icon icon="@tui.laptop" />
        <h3 tuiTitle>{{ 'Clients' | i18n }}</h3>
        <aside tuiAccessories>
          <button tuiButton iconStart="@tui.plus" (click)="onAdd('client')">
            {{ 'Add' | i18n }}
          </button>
        </aside>
      </header>
      <table tuiTable class="g-table" [tuiSkeleton]="!clients()">
        <thead>
          <tr>
            <th tuiTh [sorter]="byName">{{ 'Name' | i18n }}</th>
            <th tuiTh [sorter]="bySubnet">{{ 'Subnet' | i18n }}</th>
            <th tuiTh [sorter]="byIp">{{ 'LAN IPv4' | i18n }}</th>
            <th tuiTh>{{ 'WAN IPv4' | i18n }}</th>
            <th tuiTh>{{ 'IPv6' | i18n }}</th>
            <th tuiTh></th>
          </tr>
        </thead>
        <tbody>
          @for (device of clients() | tuiTableSort; track $index) {
            <tr>
              <td tuiTd>{{ device.name }}</td>
              <td tuiTd>{{ device.subnet.name }}</td>
              <td tuiTd>{{ device.ip }}</td>
              <td tuiTd>
                @if (device.wanIp) {
                  {{ device.wanIp }}
                } @else if (device.inheritedWan) {
                  {{ 'Subnet default' | i18n }} ({{ device.inheritedWan }})
                } @else {
                  {{ 'Subnet default' | i18n }}
                }
              </td>
              <td tuiTd>{{ device.ipv6 ?? '—' }}</td>
              <td tuiTd [style.padding-inline-end.rem]="0.625">
                <button
                  tuiIconButton
                  size="xs"
                  tuiDropdown
                  tuiDropdownAuto
                  appearance="flat-grayscale"
                  iconStart="@tui.ellipsis-vertical"
                >
                  {{ 'Actions' | i18n }}
                  <tui-data-list
                    *tuiDropdown="let close"
                    size="s"
                    (click)="close()"
                  >
                    <button
                      tuiOption
                      iconStart="@tui.pencil"
                      (click)="onEdit(device)"
                    >
                      {{ 'Edit' | i18n }}
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.settings"
                      (click)="onConfig(device)"
                    >
                      {{ 'View Config' | i18n }}
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.server"
                      (click)="onSetKind(device, 'server')"
                    >
                      {{ 'Change to Server' | i18n }}
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.trash"
                      (click)="onDelete(device)"
                    >
                      {{ 'Delete' | i18n }}
                    </button>
                  </tui-data-list>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="6">
                <app-placeholder icon="@tui.laptop">
                  {{ 'No clients' | i18n }}
                </app-placeholder>
              </td>
            </tr>
          }
        </tbody>
      </table>
    </div>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }
  `,
  imports: [
    FormsModule,
    TuiButton,
    TuiCardLarge,
    TuiDropdown,
    TuiDataList,
    TuiLoader,
    TuiSwitch,
    TuiTable,
    PlaceholderComponent,
    TuiSkeleton,
    TuiHeader,
    TuiIcon,
    TuiTitle,
    i18nPipe,
  ],
})
export default class Devices {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly errorService = inject(ErrorService)
  private readonly patch = inject<PatchDB<TunnelData>>(PatchDB)
  private readonly i18n = inject(i18nPipe)

  protected readonly togglingDns = signal<string | null>(null)
  protected readonly togglingPf = signal<string | null>(null)

  private readonly wans = toSignal(
    this.patch.watch$('gateways').pipe(map(wanOptions)),
    { initialValue: [] },
  )

  protected readonly defaultWan = toSignal(
    this.patch.watch$('gateways').pipe(map(defaultWanIp)),
    { initialValue: null },
  )

  protected readonly subnets = toSignal(
    this.patch.watch$('wg', 'subnets').pipe(
      map(subnets =>
        Object.entries(subnets).map(
          ([range, { name, clients, wanIp, ipv6 }]) => ({
            range,
            name,
            clients,
            wanIp,
            ipv6,
          }),
        ),
      ),
    ),
    { initialValue: null },
  )

  protected readonly devices = computed(() => {
    const defaultWan = this.defaultWan()

    return this.subnets()?.flatMap(subnet =>
      Object.entries(subnet.clients).map(
        ([
          ip,
          { name, kind, allowDnsInjection, allowAutoPortForward, wanIp },
        ]) => ({
          subnet: {
            name: subnet.name,
            range: subnet.range,
          },
          ip,
          name,
          kind,
          allowDnsInjection,
          allowAutoPortForward,
          wanIp,
          // Raw inherited IP; the 'Subnet default' label is translated in the
          // template so the column reacts to a live language switch.
          inheritedWan: subnet.wanIp ?? defaultWan,
          ipv6: deviceIpv6(subnet.ipv6, ip),
        }),
      ),
    )
  })

  protected readonly servers = computed(() =>
    this.devices()?.filter(d => d.kind === 'server'),
  )

  protected readonly clients = computed(() =>
    this.devices()?.filter(d => d.kind === 'client'),
  )

  protected readonly byName: TuiComparator<DeviceRow> = (a, b) =>
    (a.name || '').localeCompare(b.name || '')

  protected readonly bySubnet: TuiComparator<DeviceRow> = (a, b) =>
    a.subnet.name.localeCompare(b.subnet.name)

  protected readonly byIp: TuiComparator<DeviceRow> = (a, b) =>
    this.ip4(a.ip) - this.ip4(b.ip)

  private ip4(s: string): number {
    return (s.split('/')[0] ?? '')
      .split('.')
      .reduce((n, o) => n * 256 + Number(o), 0)
  }

  protected onAdd(kind: T.Tunnel.WgClientKind) {
    this.dialogs
      .open(DEVICES_ADD, {
        label:
          kind === 'server'
            ? this.i18n.transform('Add server')
            : this.i18n.transform('Add client'),
        data: {
          kind,
          subnets: this.subnets,
          wanOptions: this.wans(),
          defaultWan: this.defaultWan(),
        },
      })
      .subscribe()
  }

  protected onEdit(device: MappedDevice) {
    this.dialogs
      .open(DEVICES_ADD, {
        label: this.i18n.transform('Edit device'),
        data: {
          device,
          subnets: this.subnets,
          wanOptions: this.wans(),
          defaultWan: this.defaultWan(),
        },
      })
      .subscribe()
  }

  async onConfig({ subnet, ip }: MappedDevice) {
    this.tasks.run(async () => {
      const data = await this.api.showDeviceConfig({ subnet: subnet.range, ip })

      this.dialogs
        .open(DEVICES_CONFIG, { data, closable: false, size: 'm' })
        .subscribe()
    })
  }

  protected onDelete({ subnet, ip }: MappedDevice): void {
    this.dialogs
      .open(TUI_CONFIRM, { label: this.i18n.transform('Are you sure?') })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(
          async () => await this.api.deleteDevice({ subnet: subnet.range, ip }),
        ),
      )
  }

  protected async onDnsInjection({
    subnet,
    ip,
    allowDnsInjection,
  }: MappedDevice) {
    this.togglingDns.set(ip)
    try {
      await this.api.setDnsInjection({
        subnet: subnet.range,
        ip,
        enabled: !allowDnsInjection,
      })
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.togglingDns.set(null)
    }
  }

  protected async onAutoPortForward({
    subnet,
    ip,
    allowAutoPortForward,
  }: MappedDevice) {
    this.togglingPf.set(ip)
    try {
      await this.api.setAutoPortForward({
        subnet: subnet.range,
        ip,
        enabled: !allowAutoPortForward,
      })
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.togglingPf.set(null)
    }
  }

  protected onSetKind(
    { subnet, ip }: MappedDevice,
    kind: T.Tunnel.WgClientKind,
  ): void {
    this.dialogs
      .open(TUI_CONFIRM, { label: this.i18n.transform('Are you sure?') })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(
          async () =>
            await this.api.setDeviceKind({ subnet: subnet.range, ip, kind }),
        ),
      )
  }
}
