import { Component, computed, inject, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { TuiTable } from '@taiga-ui/addon-table'
import { TuiInput, TuiTextfield, TuiTitle } from '@taiga-ui/core'
import { TuiSkeleton } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { DevicesService } from 'src/app/routes/devices/service'
import { DeviceTableItem } from 'src/app/routes/devices/utils'
import { DevicesOffline } from './offline'
import { DevicesOnline } from './online'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>{{ 'Devices' | i18n }}</h2>
      </hgroup>
      <aside tuiAccessories>
        <tui-textfield tuiTextfieldSize="s" iconStart="@tui.search">
          <input
            tuiInput
            [placeholder]="'Search devices' | i18n"
            [(ngModel)]="search"
          />
        </tui-textfield>
      </aside>
    </header>
    <table
      tuiTable
      [devicesOnline]="online()"
      [tuiSkeleton]="loading()"
    ></table>
    <table
      tuiTable
      [devicesOffline]="offline()"
      [tuiSkeleton]="loading()"
    ></table>
  `,
  styles: `
    aside {
      max-width: 21rem;
      flex: 8;
    }

    tui-textfield {
      width: 100%;
    }

    table {
      ::ng-deep {
        td[colspan] {
          text-align: center;
          color: var(--tui-text-tertiary);
          padding: 0.75rem;
        }

        [tuiChip] {
          display: flex;
          background: none;
        }
      }
    }
  `,
  host: { class: 'g-page' },
  imports: [
    FormsModule,
    TuiHeader,
    TuiTitle,
    TuiTextfield,
    TuiTable,
    TuiSkeleton,
    TuiInput,
    DevicesOnline,
    DevicesOffline,
    i18nPipe,
  ],
})
export default class DevicesTable {
  protected readonly service = inject(DevicesService)
  protected readonly search = signal('')

  protected readonly loading = computed(() => !this.service.data())

  protected readonly online = computed(() =>
    this.filter(
      this.service.data()?.filter(d => d.status === 'online') ?? [],
      this.search(),
    ),
  )

  protected readonly offline = computed(() =>
    this.filter(
      this.service.data()?.filter(d => d.status === 'offline') ?? [],
      this.search(),
    ),
  )

  private filter(
    devices: readonly DeviceTableItem[],
    value: string,
  ): readonly DeviceTableItem[] {
    if (!value) return devices
    const lower = value.toLowerCase()
    return devices.filter(device =>
      [device.name, device.mac, device.ipv4, device.ipv6, device.connection]
        .filter(Boolean)
        .some(field => field!.toLowerCase().includes(lower)),
    )
  }
}
