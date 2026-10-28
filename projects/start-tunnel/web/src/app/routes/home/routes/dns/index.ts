import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  Signal,
} from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiComparator, TuiTable } from '@taiga-ui/addon-table'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TUI_CONFIRM, TuiSkeleton } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { filter, map } from 'rxjs'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { PlaceholderComponent } from 'src/app/routes/home/components/placeholder'
import { deviceIpv6 } from 'src/app/routes/home/routes/devices/utils'
import { MappedDevice } from 'src/app/routes/home/routes/published-ports/utils'
import { ApiService } from 'src/app/services/api/api.service'
import { TunnelData } from 'src/app/services/patch-db/data-model'

import { DNS_ADD } from './add'

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
      <table tuiTable class="g-table" [tuiSkeleton]="!records()">
        <thead>
          <tr>
            <th tuiTh [sorter]="byName">{{ 'Hostname' | i18n }}</th>
            <th tuiTh [sorter]="byType">{{ 'Type' | i18n }}</th>
            <th tuiTh [sorter]="byServer">{{ 'Server' | i18n }}</th>
            <th tuiTh [sorter]="byTtl">{{ 'TTL' | i18n }}</th>
            <th tuiTh></th>
          </tr>
        </thead>
        <tbody>
          @for (record of manual() | tuiTableSort; track $index) {
            <tr>
              <td tuiTd>{{ record.name }}</td>
              <td tuiTd>{{ record.type }}</td>
              <td tuiTd>{{ serverDisplay(record) }}</td>
              <td tuiTd>{{ record.ttl }}</td>
              <td tuiTd [style.padding-inline-end.rem]="0.625">
                <button
                  tuiIconButton
                  size="xs"
                  tuiDropdown
                  tuiDropdownAlign="end"
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
                      iconStart="@tui.trash"
                      (click)="onDelete(record)"
                    >
                      {{ 'Delete' | i18n }}
                    </button>
                  </tui-data-list>
                </button>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="5">
                <app-placeholder icon="@tui.list">
                  {{ 'No manual DNS records. Add one to get started.' | i18n }}
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
      <table tuiTable class="g-table no-actions" [tuiSkeleton]="!records()">
        <thead>
          <tr>
            <th tuiTh [sorter]="byName">{{ 'Hostname' | i18n }}</th>
            <th tuiTh [sorter]="byType">{{ 'Type' | i18n }}</th>
            <th tuiTh [sorter]="byServer">{{ 'Server' | i18n }}</th>
            <th tuiTh [sorter]="byTtl">{{ 'TTL' | i18n }}</th>
          </tr>
        </thead>
        <tbody>
          @for (record of automatic() | tuiTableSort; track $index) {
            <tr>
              <td tuiTd>{{ record.name }}</td>
              <td tuiTd>{{ record.type }}</td>
              <td tuiTd>{{ serverDisplay(record) }}</td>
              <td tuiTd>{{ record.ttl }}</td>
            </tr>
          } @empty {
            <tr>
              <td colspan="4">
                <app-placeholder icon="@tui.list">
                  {{
                    'No automatic DNS records. Devices you trust can add their own via RFC 2136.'
                      | i18n
                  }}
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
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    TuiButton,
    TuiCardLarge,
    TuiDropdown,
    TuiDataList,
    TuiTable,
    PlaceholderComponent,
    TuiSkeleton,
    TuiHeader,
    TuiIcon,
    TuiTitle,
    i18nPipe,
  ],
})
export default class Dns {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly patch = inject<PatchDB<TunnelData>>(PatchDB)
  private readonly i18n = inject(i18nPipe)

  protected readonly records = toSignal(this.patch.watch$('dnsRecords'))
  protected readonly manual = computed(() =>
    (this.records() || []).filter(r => r.source === null),
  )
  protected readonly automatic = computed(() =>
    (this.records() || []).filter(r => r.source !== null),
  )

  private readonly devices = toSignal(
    this.patch.watch$('wg', 'subnets').pipe(
      map(subnets =>
        Object.values(subnets).flatMap(({ clients, ipv6 }) =>
          Object.entries(clients).map(([ip, { name, kind }]) => ({
            ip,
            name,
            kind,
            ipv6: deviceIpv6(ipv6, ip),
          })),
        ),
      ),
    ),
    { initialValue: [] },
  )

  protected readonly servers: Signal<MappedDevice[]> = computed(() =>
    this.devices().filter(device => device.kind === 'server'),
  )

  protected readonly byName: TuiComparator<T.Tunnel.DnsRecordEntry> = (a, b) =>
    (a.name || '').localeCompare(b.name || '')
  protected readonly byType: TuiComparator<T.Tunnel.DnsRecordEntry> = (a, b) =>
    (a.type || '').localeCompare(b.type || '')
  protected readonly byServer: TuiComparator<T.Tunnel.DnsRecordEntry> = (
    a,
    b,
  ) => this.serverDisplay(a).localeCompare(this.serverDisplay(b))
  protected readonly byTtl: TuiComparator<T.Tunnel.DnsRecordEntry> = (a, b) =>
    a.ttl - b.ttl

  protected serverDisplay(record: {
    type: string
    source: string | null
    value: string
  }): string {
    if (record.type !== 'A' && record.type !== 'AAAA') return record.value

    const ip = record.source ?? record.value
    const name = this.devices().find(d => d.ip === ip || d.ipv6 === ip)?.name
    return name ? `${name} (${record.value})` : record.value
  }

  protected onAdd(): void {
    this.dialogs
      .open(DNS_ADD, {
        label: this.i18n.transform('Add DNS record'),
        data: { devices: this.servers },
      })
      .subscribe()
  }

  protected onDelete(record: { name: string; type: string }): void {
    this.dialogs
      .open(TUI_CONFIRM, { label: this.i18n.transform('Are you sure?') })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(
          async () =>
            await this.api.rpc.dns.remove({
              name: record.name,
              type: record.type,
            }),
        ),
      )
  }
}
