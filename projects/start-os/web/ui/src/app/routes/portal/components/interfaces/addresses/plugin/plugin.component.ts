import { Component, computed, inject, input } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { TuiAvatar } from '@taiga-ui/kit'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { PkgIconDirective } from 'src/app/routes/portal/components/pkg-icon'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { ActionService } from 'src/app/services/action.service'
import {
  MappedServiceInterface,
  PluginAddressGroup,
} from '../../interface.service'
import {
  HTTP_PROXY_PLUGINS,
  HttpProxyChainButton,
  showsHttpProxyChain,
} from '../../proxy-chain'
import { PluginItemComponent } from './item.component'

@Component({
  selector: 'section[pluginGroup]',
  template: `
    <header tuiHeader="body-l">
      @if (pluginGroup().pluginPkgInfo; as pkgInfo) {
        <span tuiAvatar size="xs">
          <img [pkgIcon]="pkgInfo.id" />
        </span>
      }
      <span tuiTitle>{{ pluginGroup().pluginName }}</span>
      <aside tuiAccessories>
        @if (value(); as iface) {
          @if (showsChain(iface.binding)) {
            <button
              [httpProxyChain]="chainGateway()"
              [gatewayName]="pluginGroup().pluginName"
              [address]="iface.addressInfo"
              [packageId]="packageId()"
            ></button>
          }
        }
        @if (pluginGroup().tableAction; as action) {
          <button tuiButton iconStart="@tui.plus" (click)="runTableAction()">
            {{ action.metadata.name }}
          </button>
        }
      </aside>
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
    :host-context(tui-root._mobile) header button {
      font-size: 0;
      gap: 0;
    }
  `,
  host: { class: 'g-card' },
  imports: [
    TuiButton,
    TuiHeader,
    TuiTitle,
    TableComponent,
    PlaceholderComponent,
    i18nPipe,
    PluginItemComponent,
    TuiAvatar,
    PkgIconDirective,
    HttpProxyChainButton,
  ],
})
export class PluginAddressesComponent {
  private readonly actionService = inject(ActionService)
  private readonly proxyPlugins = inject(HTTP_PROXY_PLUGINS)

  readonly pluginGroup = input.required<PluginAddressGroup>()
  readonly packageId = input('')
  readonly value = input<MappedServiceInterface | undefined>()

  protected readonly chainGateway = computed<T.HttpProxyGateway>(() => ({
    kind: 'url-plugin',
    packageId: this.pluginGroup().pluginId,
  }))

  protected showsChain(binding: T.BindInfo | undefined): boolean {
    return showsHttpProxyChain(
      binding,
      this.chainGateway(),
      this.proxyPlugins().length,
    )
  }

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
