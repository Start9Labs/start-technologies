import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { InterfaceComponent } from 'src/app/routes/portal/components/interfaces/interface.component'
import { InterfaceService } from 'src/app/routes/portal/components/interfaces/interface.service'
import { GatewayService } from 'src/app/services/gateway.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'

@Component({
  template: `
    <ng-container *title>
      <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
        {{ 'Back' | i18n }}
      </a>
      {{ iface.name }}
    </ng-container>
    <header tuiHeader>
      <hgroup tuiTitle>
        <h3>
          {{ iface.name }}
        </h3>
        <p tuiSubtitle>{{ iface.description }}</p>
      </hgroup>
    </header>
    <br />
    <service-interface packageId="start-os" [value]="ui()" [isRunning]="true" />
  `,
  host: { class: 'g-subpage' },
  providers: [GatewayService],
  imports: [
    InterfaceComponent,
    RouterLink,
    TuiButton,
    TitleDirective,
    TuiHeader,
    TuiTitle,
    i18nPipe,
  ],
})
export default class StartOsUiComponent {
  private readonly interfaceService = inject(InterfaceService)
  private readonly gatewayService = inject(GatewayService)
  private readonly i18n = inject(i18nPipe)

  readonly iface: T.ServiceInterface = {
    id: 'admin-ui',
    name: 'StartOS UI',
    description: this.i18n.transform(
      'Your personal StartOS web user interface.',
    )!,
    type: 'ui' as const,
    masked: false,
    addressInfo: {
      hostId: 'admin',
      internalPort: 80,
      scheme: 'http',
      sslScheme: 'https',
      suffix: '',
      username: null,
    },
  }

  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  readonly network = toSignal(this.patch.watch$('serverInfo', 'network'))

  readonly allPackageData = toSignal(this.patch.watch$('packageData'))

  readonly ui = computed(() => {
    const network = this.network()
    const gateways = this.gatewayService.gateways()

    if (!network || !gateways) return

    return {
      ...this.iface,
      gatewayGroups: this.interfaceService.getGatewayGroups(
        this.iface,
        network.host,
        gateways,
      ),
      pluginGroups: this.interfaceService.getPluginGroups(
        this.iface,
        network.host,
        this.allPackageData(),
      ),
      addSsl: true,
      sharedHostNames: [],
    }
  })
}
