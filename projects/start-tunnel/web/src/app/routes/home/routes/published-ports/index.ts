import { Component, computed, inject, signal, Signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { ErrorService, TaskService } from '@start9labs/shared'
import { utils } from '@start9labs/start-core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiComparator, TuiTable } from '@taiga-ui/addon-table'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiLoader,
  TuiTextfield,
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
import { PUBLISHED_PORTS_ADD } from 'src/app/routes/home/routes/published-ports/add'
import { PUBLISHED_PORTS_EDIT_LABEL } from 'src/app/routes/home/routes/published-ports/edit-label'
import { deviceIpv6 } from 'src/app/routes/home/routes/devices/utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { ApiService } from 'src/app/services/api/api.service'
import { TunnelData } from 'src/app/services/patch-db/data-model'

import { mapForwards, mapPinholes, MappedDevice, MappedForward } from './utils'

@Component({
  template: `
    <div tuiCardLarge="compact" appearance="floating">
      <header tuiHeader="body-l">
        <tui-icon icon="@tui.pencil" />
        <h3 tuiTitle>{{ 'Manual' | i18n }}</h3>
        <aside tuiAccessories>
          <button tuiButton iconStart="@tui.plus" (click)="onAdd()">
            {{ 'Add' | i18n }}
          </button>
        </aside>
      </header>
      <table tuiTable class="g-table" [tuiSkeleton]="!portForwards()">
        <thead>
          <tr>
            <th tuiTh></th>
            <th tuiTh [sorter]="byLabel">{{ 'Label' | i18n }}</th>
            <th tuiTh [sorter]="byServer">{{ 'Server' | i18n }}</th>
            <th tuiTh [sorter]="byHostname">{{ 'Hostname' | i18n }}</th>
            <th tuiTh [sorter]="byExternalPort">
              {{ 'External Port' | i18n }}
            </th>
            <th tuiTh [sorter]="byInternalPort">
              {{ 'Internal Port' | i18n }}
            </th>
            <th tuiTh>{{ 'Protocol' | i18n }}</th>
            <th tuiTh [sorter]="byIp">{{ 'IP' | i18n }}</th>
            <th tuiTh></th>
          </tr>
        </thead>
        <tbody>
          @for (forward of manual() | tuiTableSort; track $index) {
            <tr>
              <td tuiTd>
                <tui-loader
                  size="xs"
                  [loading]="toggling() === key(forward)"
                  [overlay]="true"
                >
                  <input
                    tuiSwitch
                    type="checkbox"
                    size="s"
                    [style.display]="'flex'"
                    [showIcons]="false"
                    [ngModel]="forward.enabled"
                    (ngModelChange)="onToggle(forward)"
                  />
                </tui-loader>
              </td>
              <td tuiTd>{{ forward.label || '—' }}</td>
              <td tuiTd>{{ forward.device.name }}</td>
              <td tuiTd>{{ forward.sni || '—' }}</td>
              <td tuiTd>{{ span(forward.externalport, forward.count) }}</td>
              <td tuiTd>{{ span(forward.internalport, forward.count) }}</td>
              <td tuiTd>{{ 'TCP/UDP' | i18n }}</td>
              <td tuiTd>{{ forward.externalip }}</td>
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
                      (click)="onEditLabel(forward)"
                    >
                      {{
                        forward.label ? ('Rename' | i18n) : ('Add label' | i18n)
                      }}
                    </button>
                    <button
                      tuiOption
                      iconStart="@tui.trash"
                      (click)="onDelete(forward)"
                    >
                      {{ 'Delete' | i18n }}
                    </button>
                  </tui-data-list>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="9">
                <app-placeholder icon="@tui.globe">
                  {{ 'No published ports' | i18n }}
                </app-placeholder>
              </td>
            </tr>
          }
        </tbody>
      </table>
    </div>

    <div tuiCardLarge="compact" appearance="floating">
      <header tuiHeader="body-l">
        <tui-icon icon="@tui.zap" />
        <h3 tuiTitle>{{ 'Automatic' | i18n }}</h3>
      </header>
      <table
        tuiTable
        class="g-table no-actions"
        [tuiSkeleton]="!portForwards()"
      >
        <thead>
          <tr>
            <th tuiTh [sorter]="byServer">{{ 'Server' | i18n }}</th>
            <th tuiTh [sorter]="byHostname">{{ 'Hostname' | i18n }}</th>
            <th tuiTh [sorter]="byExternalPort">
              {{ 'External Port' | i18n }}
            </th>
            <th tuiTh [sorter]="byInternalPort">
              {{ 'Internal Port' | i18n }}
            </th>
            <th tuiTh>{{ 'Protocol' | i18n }}</th>
            <th tuiTh [sorter]="byIp">{{ 'IP' | i18n }}</th>
          </tr>
        </thead>
        <tbody>
          @for (forward of automatic() | tuiTableSort; track $index) {
            <tr>
              <td tuiTd>{{ forward.device.name }}</td>
              <td tuiTd>{{ forward.sni || '—' }}</td>
              <td tuiTd>{{ span(forward.externalport, forward.count) }}</td>
              <td tuiTd>{{ span(forward.internalport, forward.count) }}</td>
              <td tuiTd>{{ 'TCP/UDP' | i18n }}</td>
              <td tuiTd>{{ forward.externalip }}</td>
            </tr>
          } @empty {
            <tr>
              <td colspan="6">
                <app-placeholder icon="@tui.globe">
                  {{ 'No published ports' | i18n }}
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
    TuiTable,
    TuiDropdown,
    TuiDataList,
    TuiLoader,
    TuiSwitch,
    TuiTextfield,
    PlaceholderComponent,
    TuiSkeleton,
    TuiHeader,
    TuiIcon,
    TuiTitle,
    i18nPipe,
  ],
})
export default class PublishedPorts {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly patch = inject<PatchDB<TunnelData>>(PatchDB)
  private readonly errorService = inject(ErrorService)
  private readonly i18n = inject(i18nPipe)
  private readonly ips = toSignal(
    this.patch.watch$('gateways').pipe(
      map(g =>
        Object.values(g)
          .flatMap(
            val => val.ipInfo?.subnets.map(s => utils.IpNet.parse(s)) || [],
          )
          .filter(s => s.isIpv4() && s.isPublic())
          .map(s => s.address),
      ),
    ),
    { initialValue: [] },
  )

  // Only servers can receive forwards, so the picker lists servers only. Each
  // carries its computed IPv6 GUA (null when the subnet has no v6 prefix).
  private readonly devices: Signal<MappedDevice[]> = toSignal(
    this.patch.watch$('wg', 'subnets').pipe(
      map(subnets =>
        Object.values(subnets).flatMap(subnet =>
          Object.entries(subnet.clients)
            .filter(([, c]) => c.kind === 'server')
            .map(([ip, { name }]) => ({
              ip,
              name,
              ipv6: deviceIpv6(subnet.ipv6, ip),
            })),
        ),
      ),
    ),
    { initialValue: [] },
  )

  // All devices (any kind), so rows targeting a client still resolve a name.
  private readonly allDevices: Signal<MappedDevice[]> = toSignal(
    this.patch.watch$('wg', 'subnets').pipe(
      map(subnets =>
        Object.values(subnets).flatMap(subnet =>
          Object.entries(subnet.clients).map(([ip, { name }]) => ({
            ip,
            name,
            ipv6: deviceIpv6(subnet.ipv6, ip),
          })),
        ),
      ),
    ),
    { initialValue: [] },
  )

  protected readonly portForwards = toSignal(this.patch.watch$('portForwards'))
  protected readonly pinholes = toSignal(this.patch.watch$('pinholes6'))
  protected readonly forwards = computed(() => [
    ...mapForwards(this.portForwards() || {}, this.allDevices()),
    ...mapPinholes(this.pinholes() || {}, this.allDevices()),
  ])
  protected readonly manual = computed(() =>
    this.forwards().filter(f => !f.auto),
  )
  protected readonly automatic = computed(() =>
    this.forwards().filter(f => f.auto),
  )

  protected readonly toggling = signal<string | null>(null)

  protected readonly byLabel: TuiComparator<MappedForward> = (a, b) =>
    (a.label || '').localeCompare(b.label || '')
  protected readonly byIp: TuiComparator<MappedForward> = (a, b) =>
    a.externalip.localeCompare(b.externalip)
  protected readonly byHostname: TuiComparator<MappedForward> = (a, b) =>
    (a.sni || '').localeCompare(b.sni || '')
  protected readonly byExternalPort: TuiComparator<MappedForward> = (a, b) =>
    Number(a.externalport) - Number(b.externalport)
  protected readonly byServer: TuiComparator<MappedForward> = (a, b) =>
    a.device.name.localeCompare(b.device.name)
  protected readonly byInternalPort: TuiComparator<MappedForward> = (a, b) =>
    Number(a.internalport) - Number(b.internalport)

  protected key(forward: MappedForward): string {
    return `${forward.ipVersion}:${forward.externalip}:${forward.externalport}:${forward.hostname ?? ''}`
  }

  // Renders a forwarded port span: a single port when count is 1, else `start-end`.
  protected span(startPort: string, count: number): string {
    const start = Number(startPort)
    return count > 1 ? `${start}-${start + count - 1}` : startPort
  }

  protected async onToggle(forward: MappedForward) {
    this.toggling.set(this.key(forward))

    try {
      if (forward.ipVersion === 'ipv6') {
        await this.api.setPinholeEnabled({
          gua: forward.externalip,
          externalPort: Number(forward.externalport),
          enabled: !forward.enabled,
        })
      } else {
        await this.api.setForwardEnabled({
          source: `${forward.externalip}:${forward.externalport}`,
          enabled: !forward.enabled,
          hostname: forward.hostname,
        })
      }
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.toggling.set(null)
    }
  }

  protected onAdd(): void {
    this.dialogs
      .open(PUBLISHED_PORTS_ADD, {
        label: this.i18n.transform('Add published port'),
        data: { ips: this.ips, devices: this.devices },
      })
      .subscribe()
  }

  protected onEditLabel(forward: MappedForward): void {
    this.dialogs
      .open(PUBLISHED_PORTS_EDIT_LABEL, {
        label: this.i18n.transform('Edit label'),
        data: {
          source: `${forward.externalip}:${forward.externalport}`,
          label: forward.label,
          hostname: forward.hostname,
          pinhole:
            forward.ipVersion === 'ipv6'
              ? {
                  gua: forward.externalip,
                  externalPort: Number(forward.externalport),
                }
              : undefined,
        },
      })
      .subscribe()
  }

  protected onDelete(forward: MappedForward): void {
    this.dialogs
      .open(TUI_CONFIRM, { label: this.i18n.transform('Are you sure?') })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(async () => {
          if (forward.ipVersion === 'ipv6') {
            await this.api.deletePinhole({
              gua: forward.externalip,
              externalPort: Number(forward.externalport),
            })
          } else {
            await this.api.deleteForward({
              source: `${forward.externalip}:${forward.externalport}`,
              hostname: forward.hostname,
            })
          }
        }),
      )
  }
}
