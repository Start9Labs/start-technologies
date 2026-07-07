import { inject, Injectable } from '@angular/core'
import { DialogService, ErrorService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { dnsAllPass, getGua, portAllPass } from 'src/app/utils/gua'
import { DOMAIN_VALIDATION, DnsGateway } from './dns.component'
import { PORT_FORWARD_VALIDATION } from './port-forward.component'
import { PRIVATE_DNS_VALIDATION } from './private-dns.component'

@Injectable({ providedIn: 'root' })
export class DomainHealthService {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly dialog = inject(DialogService)
  private readonly api = inject(ApiService)
  private readonly errorService = inject(ErrorService)

  async checkPublicDomain(
    fqdn: string,
    gatewayId: string,
    portOrRes: number | T.AddPublicDomainRes,
    count = 1,
  ): Promise<void> {
    try {
      const gateway = await this.getGatewayData(gatewayId)
      if (!gateway) return

      // A port range can't be reachability-tested a port at a time, so we only
      // verify DNS and never claim its port forward is (or isn't) open.
      const isRange = count > 1
      const gua = getGua(gateway.ipInfo)

      let dns: T.QueryDnsRes | null
      let port: number
      let portResult: T.CheckPortRes | null
      let portV6Result: T.CheckPortV6Res | null

      if (typeof portOrRes === 'number') {
        port = portOrRes
        const [dnsRes, portRes, portV6Res] = await Promise.all([
          this.api.queryDns({ fqdn }).catch((): null => null),
          isRange
            ? Promise.resolve(null)
            : this.api
                .checkPort({ gateway: gatewayId, port: portOrRes })
                .catch((): null => null),
          isRange || !gua
            ? Promise.resolve(null)
            : this.api
                .checkPortV6({ gateway: gatewayId, port: portOrRes })
                .catch((): null => null),
        ])
        dns = dnsRes
        portResult = portRes
        portV6Result = portV6Res
      } else {
        dns = portOrRes.dns
        port = portOrRes.port.port
        portResult = isRange ? null : portOrRes.port
        portV6Result = isRange ? null : portOrRes.portV6
      }

      const dnsPass = dnsAllPass(dns, gateway.ipInfo.wanIp, gua)
      const portOk = isRange || portAllPass(portResult, portV6Result, gua)

      if (!dnsPass || !portOk) {
        setTimeout(
          () =>
            this.openPublicDomainModal(fqdn, gateway, port, count, {
              dns,
              portResult,
              portV6Result,
            }),
          250,
        )
      }
    } catch (e: any) {
      this.errorService.handleError(e)
    }
  }

  async checkPrivateDomain(
    gatewayId: string,
    fqdn: string,
    prefetchedConfigured?: boolean,
  ): Promise<void> {
    try {
      const gateway = await this.getGatewayData(gatewayId)
      if (!gateway) return

      const configured =
        prefetchedConfigured ??
        (await this.api
          .checkDns({ gateway: gatewayId, fqdn })
          .catch(() => false))

      if (!configured) {
        setTimeout(
          () => this.openPrivateDomainModal(gateway, fqdn, { configured }),
          250,
        )
      }
    } catch (e: any) {
      this.errorService.handleError(e)
    }
  }

  async showPublicDomainSetup(
    fqdn: string,
    gatewayId: string,
    port: number,
    count = 1,
  ): Promise<void> {
    try {
      const gateway = await this.getGatewayData(gatewayId)
      if (!gateway) return

      this.openPublicDomainModal(fqdn, gateway, port, count)
    } catch (e: any) {
      this.errorService.handleError(e)
    }
  }

  async checkPortForward(gatewayId: string, port: number): Promise<void> {
    try {
      const gateway = await this.getGatewayData(gatewayId)
      if (!gateway) return

      const portResult = await this.api
        .checkPort({ gateway: gatewayId, port })
        .catch((): null => null)

      const portOk =
        !!portResult?.openInternally &&
        !!portResult?.openExternally &&
        !!portResult?.hairpinning

      if (!portOk) {
        setTimeout(
          () => this.openPortForwardModal(gateway, port, 1, { portResult }),
          250,
        )
      }
    } catch (e: any) {
      this.errorService.handleError(e)
    }
  }

  async showPortForwardSetup(
    gatewayId: string,
    port: number,
    count = 1,
  ): Promise<void> {
    try {
      const gateway = await this.getGatewayData(gatewayId)
      if (!gateway) return

      this.openPortForwardModal(gateway, port, count)
    } catch (e: any) {
      this.errorService.handleError(e)
    }
  }

  async showPrivateDomainSetup(gatewayId: string, fqdn: string): Promise<void> {
    try {
      const gateway = await this.getGatewayData(gatewayId)
      if (!gateway) return

      this.openPrivateDomainModal(gateway, fqdn)
    } catch (e: any) {
      this.errorService.handleError(e)
    }
  }

  private async getGatewayData(gatewayId: string): Promise<DnsGateway | null> {
    const network = await firstValueFrom(
      this.patch.watch$('serverInfo', 'network'),
    )
    const gateway = network.gateways[gatewayId]
    if (!gateway?.ipInfo) return null
    return { id: gatewayId, ...gateway, ipInfo: gateway.ipInfo }
  }

  private openPublicDomainModal(
    fqdn: string,
    gateway: DnsGateway,
    port: number,
    count: number,
    initialResults?: {
      dns: T.QueryDnsRes | null
      portResult: T.CheckPortRes | null
      portV6Result: T.CheckPortV6Res | null
    },
  ) {
    this.dialog
      .openComponent(DOMAIN_VALIDATION, {
        label: 'Address Requirements',
        size: 'm',
        data: { fqdn, gateway, port, count, initialResults },
      })
      .subscribe()
  }

  private openPortForwardModal(
    gateway: DnsGateway,
    port: number,
    count: number,
    initialResults?: { portResult: T.CheckPortRes | null },
  ) {
    this.dialog
      .openComponent(PORT_FORWARD_VALIDATION, {
        label: 'Address Requirements',
        size: 'm',
        data: { gateway, port, count, initialResults },
      })
      .subscribe()
  }

  private openPrivateDomainModal(
    gateway: DnsGateway,
    fqdn: string,
    initialResults?: { configured: boolean },
  ) {
    this.dialog
      .openComponent(PRIVATE_DNS_VALIDATION, {
        label: 'Address Requirements',
        size: 'm',
        data: { gateway, fqdn, initialResults },
      })
      .subscribe()
  }
}
