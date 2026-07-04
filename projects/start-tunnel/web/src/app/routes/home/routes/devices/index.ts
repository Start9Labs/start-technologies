import { Component, computed, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { ErrorService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiLoader,
  TuiTitle,
} from '@taiga-ui/core'
import {
  TUI_CONFIRM,
  TuiNotificationMiddleService,
  TuiSkeleton,
  TuiSwitch,
} from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { filter, map } from 'rxjs'
import { PlaceholderComponent } from 'src/app/routes/home/components/placeholder'
import {
  defaultWanIp,
  wanLabel,
  wanOptions,
} from 'src/app/routes/home/components/wan'
import { ApiService } from 'src/app/services/api/api.service'
import { TunnelData } from 'src/app/services/patch-db/data-model'
import { DEVICES_ADD } from './add'
import { DEVICES_CONFIG } from './config'
import { deviceIpv6, MappedDevice } from './utils'

@Component({
  template: `
    <div tuiCardLarge="compact" appearance="floating">
      <header tuiHeader="body-l">
        <tui-icon icon="@tui.server" />
        <h3 tuiTitle>Servers</h3>
        <aside tuiAccessories>
          <button tuiButton iconStart="@tui.plus" (click)="onAdd('server')">
            Add
          </button>
        </aside>
      </header>
      <table class="g-table" [tuiSkeleton]="!servers()">
        <thead>
          <tr>
            <th>Name</th>
            <th>Subnet</th>
            <th>LAN IPv4</th>
            <th>DNS Injection</th>
            <th>Auto Port Forward</th>
            <th>WAN IPv4</th>
            <th>IPv6</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          @for (device of servers(); track $index) {
            <tr>
              <td>{{ device.name }}</td>
              <td>{{ device.subnet.name }}</td>
              <td>{{ device.ip }}</td>
              <td>
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
              <td>
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
              <td>{{ device.wan }}</td>
              <td>{{ device.ipv6 ?? '—' }}</td>
              <td [style.padding-inline-end.rem]="0.625">
                <button
                  tuiIconButton
                  size="xs"
                  tuiDropdown
                  tuiDropdownAuto
                  appearance="flat-grayscale"
                  iconStart="@tui.ellipsis-vertical"
                >
                  Actions
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
                      Edit
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.settings"
                      (click)="onConfig(device)"
                    >
                      View Config
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.laptop"
                      (click)="onSetKind(device, 'client')"
                    >
                      Change to Client
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.trash"
                      (click)="onDelete(device)"
                    >
                      Delete
                    </button>
                  </tui-data-list>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="8">
                <app-placeholder icon="@tui.laptop">No servers</app-placeholder>
              </td>
            </tr>
          }
        </tbody>
      </table>
    </div>

    <div tuiCardLarge="compact" appearance="floating">
      <header tuiHeader="body-l">
        <tui-icon icon="@tui.laptop" />
        <h3 tuiTitle>Clients</h3>
        <aside tuiAccessories>
          <button tuiButton iconStart="@tui.plus" (click)="onAdd('client')">
            Add
          </button>
        </aside>
      </header>
      <table class="g-table" [tuiSkeleton]="!clients()">
        <thead>
          <tr>
            <th>Name</th>
            <th>Subnet</th>
            <th>LAN IPv4</th>
            <th>WAN IPv4</th>
            <th>IPv6</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          @for (device of clients(); track $index) {
            <tr>
              <td>{{ device.name }}</td>
              <td>{{ device.subnet.name }}</td>
              <td>{{ device.ip }}</td>
              <td>{{ device.wan }}</td>
              <td>{{ device.ipv6 ?? '—' }}</td>
              <td [style.padding-inline-end.rem]="0.625">
                <button
                  tuiIconButton
                  size="xs"
                  tuiDropdown
                  tuiDropdownAuto
                  appearance="flat-grayscale"
                  iconStart="@tui.ellipsis-vertical"
                >
                  Actions
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
                      Edit
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.settings"
                      (click)="onConfig(device)"
                    >
                      View Config
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.server"
                      (click)="onSetKind(device, 'server')"
                    >
                      Change to Server
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.trash"
                      (click)="onDelete(device)"
                    >
                      Delete
                    </button>
                  </tui-data-list>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="6">
                <app-placeholder icon="@tui.laptop">No clients</app-placeholder>
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
    PlaceholderComponent,
    TuiSkeleton,
    TuiHeader,
    TuiIcon,
    TuiTitle,
  ],
})
export default class Devices {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly api = inject(ApiService)
  private readonly loading = inject(TuiNotificationMiddleService)
  private readonly errorService = inject(ErrorService)
  private readonly patch = inject<PatchDB<TunnelData>>(PatchDB)

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
          wan: wanLabel(wanIp, 'Subnet default', subnet.wanIp ?? defaultWan),
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

  protected onAdd(kind: T.Tunnel.WgClientKind) {
    this.dialogs
      .open(DEVICES_ADD, {
        label: kind === 'server' ? 'Add server' : 'Add client',
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
        label: 'Edit device',
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
    const loader = this.loading.open('').subscribe()
    try {
      const data = await this.api.showDeviceConfig({ subnet: subnet.range, ip })

      this.dialogs
        .open(DEVICES_CONFIG, { data, closable: false, size: 'm' })
        .subscribe()
    } catch (e: any) {
      console.log(e)
      this.errorService.handleError(e)
    } finally {
      loader.unsubscribe()
    }
  }

  protected onDelete({ subnet, ip }: MappedDevice): void {
    this.dialogs
      .open(TUI_CONFIRM, { label: 'Are you sure?' })
      .pipe(filter(Boolean))
      .subscribe(async () => {
        const loader = this.loading.open('').subscribe()
        try {
          await this.api.deleteDevice({ subnet: subnet.range, ip })
        } catch (e: any) {
          this.errorService.handleError(e)
          console.log(e)
        } finally {
          loader.unsubscribe()
        }
      })
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
      .open(TUI_CONFIRM, { label: 'Are you sure?' })
      .pipe(filter(Boolean))
      .subscribe(async () => {
        const loader = this.loading.open('').subscribe()
        try {
          await this.api.setDeviceKind({ subnet: subnet.range, ip, kind })
        } catch (e: any) {
          this.errorService.handleError(e)
          console.log(e)
        } finally {
          loader.unsubscribe()
        }
      })
  }
}
