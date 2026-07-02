import { Component, inject, input } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'
import { TuiAvatar } from '@taiga-ui/kit'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { ActionService } from 'src/app/services/action.service'
import {
  MappedServiceInterface,
  PluginAddressGroup,
} from '../../interface.service'
import { PluginItemComponent } from './item.component'

@Component({
  selector: 'section[pluginGroup]',
  template: `
    <header>
      @if (pluginGroup().pluginPkgInfo; as pkgInfo) {
        <span tuiAvatar size="xs">
          <img [src]="pkgInfo.icon" alt="" />
        </span>
      }
      {{ pluginGroup().pluginName }}
      @if (pluginGroup().tableAction; as action) {
        <button
          tuiButton
          iconStart="@tui.plus"
          [style.margin-inline-start]="'auto'"
          (click)="runTableAction()"
        >
          {{ action.metadata.name }}
        </button>
      }
    </header>
    <table [appTable]="['Protocol', 'URL', null]">
      @for (address of pluginGroup().addresses; track $index) {
        <tr
          pluginAddress
          [address]="address"
          [pluginGroup]="pluginGroup()"
          [packageId]="packageId()"
          [value]="value()"
        ></tr>
      } @empty {
        <tr>
          <td colspan="3">
            <app-placeholder icon="@tui.list-x">
              {{ 'No addresses' | i18n }}
            </app-placeholder>
          </td>
        </tr>
      }
    </table>
  `,
  styles: `
    :host ::ng-deep {
      th:first-child {
        width: 5rem;
      }
    }

    @media (max-width: 450px) {
      header button {
        font-size: 0;
        gap: 0;
      }
    }
  `,
  host: { class: 'g-card' },
  imports: [
    TuiButton,
    TableComponent,
    PlaceholderComponent,
    i18nPipe,
    PluginItemComponent,
    TuiAvatar,
  ],
})
export class PluginAddressesComponent {
  private readonly actionService = inject(ActionService)

  readonly pluginGroup = input.required<PluginAddressGroup>()
  readonly packageId = input('')
  readonly value = input<MappedServiceInterface | undefined>()

  runTableAction() {
    const group = this.pluginGroup()
    if (!group.tableAction || !group.pluginPkgInfo) return

    const iface = this.value()
    if (!iface) return

    const { addressInfo } = iface

    this.actionService.present({
      pkgInfo: group.pluginPkgInfo,
      actionInfo: group.tableAction,
      prefill: {
        urlPluginMetadata: {
          packageId: this.packageId(),
          hostId: addressInfo.hostId,
          interfaceId: iface.id,
          internalPort: addressInfo.internalPort,
        },
      },
    })
  }
}
