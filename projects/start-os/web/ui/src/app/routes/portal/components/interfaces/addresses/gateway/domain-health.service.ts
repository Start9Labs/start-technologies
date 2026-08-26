import { inject, Injectable } from '@angular/core'
import { DialogService, ErrorService, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom, Observable, Subscription } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import {
  DataModel,
  PackageDataEntry,
} from 'src/app/services/patch-db/data-model'
import { renderPkgStatus } from 'src/app/services/pkg-status-rendering.service'
import {
  dnsAllPass,
  externalAllPass,
  getGua,
  portAllPass,
} from 'src/app/utils/gua'
import { GatewayAddress, MappedServiceInterface } from '../../interface.service'
import {
  CHALLENGE_PORT,
  DOMAIN_VALIDATION,
  DnsGateway,
  DomainValidationData,
} from './domain-validation.component'
import { PORT_FORWARD_VALIDATION } from './port-forward.component'
import { PRIVATE_DNS_VALIDATION } from './private-dns.component'

// The service context an address check runs in. `packageId`/`addSsl` make the
// Address Requirements modal status-aware (an empty `packageId` has no service
// to watch); `watch`, present only for auto-checks fired on add/enable, reports
// whether the service was non-running while the request was in flight.
export type AddressCheckContext = {
  packageId: string
  addSsl: boolean
  // The domain's ACME authority. Such a domain also needs 443 reachable.
  acme?: T.AcmeProvider | null
  watch?: ServiceStatusWatch
}

// Records whether a package was outside the 'running' state at any point between
// construction and `stop()`. Deliberately not a running -> non-running
// *transition*: if the service was down for any part of the window — restarted
// by the change, or intentionally stopped the whole time — nothing was
// listening, so an external probe cannot tell "forwarding is broken" from
// "there was nothing to forward to". A `null` source (an empty package id) never
// reports non-running. Start it just before the mutating request so the whole
// request/response cycle is covered.
export class ServiceStatusWatch {
  private sawNonRunning = false
  private readonly sub: Subscription

  constructor(status$: Observable<PackageDataEntry | undefined> | null) {
    this.sub =
      status$?.subscribe(pkg => {
        if (pkg && renderPkgStatus(pkg).primary !== 'running') {
          this.sawNonRunning = true
        }
      }) ?? Subscription.EMPTY
  }

  get wasNonRunning(): boolean {
    return this.sawNonRunning
  }

  stop(): void {
    this.sub.unsubscribe()
  }
}

@Injectable({ providedIn: 'root' })
export class DomainHealthService {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly dialog = inject(DialogService)
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly errorService = inject(ErrorService)

  // Begin watching a service's status. Call this immediately before the request
  // that adds a domain or enables an address, then hand the result to the check
  // so any downtime during that request is caught. Caller owns `stop()`.
  watchServiceStatus(packageId: string): ServiceStatusWatch {
    return new ServiceStatusWatch(
      packageId ? this.patch.watch$('packageData', packageId) : null,
    )
  }

  // Enable or disable one address, then — on enable — run its reachability
  // checks under a status watch (so any downtime during the change is caught,
  // see checkPublicDomain / checkPortForward). Shared by the desktop row switch
  // and the mobile actions dropdown so both platforms surface the setup modal.
  async setAddressEnabled(
    enabled: boolean,
    addr: GatewayAddress,
    iface: MappedServiceInterface,
    packageId: string,
    gatewayId: string,
  ): Promise<void> {
    const params = {
      internalPort: iface.addressInfo.internalPort,
      address: addr.hostnameInfo,
      enabled,
      package: packageId,
      host: iface.addressInfo.hostId,
    }

    await this.tasks.run(async () => {
      const watch = this.watchServiceStatus(packageId)
      try {
        if (packageId) {
          // A range spans >1 port and lives in a separate subtree, so it has its
          // own endpoint; a single-port binding is exactly 1.
          if (addr.count > 1) {
            await this.api.pkgBindingSetRangeAddressEnabled(params)
          } else {
            await this.api.pkgBindingSetAddressEnabled(params)
          }
        } else {
          await this.api.serverBindingSetAddressEnabled({
            internalPort: 80,
            address: addr.hostnameInfo,
            enabled,
          })
        }

        if (!enabled) return

        const kind = addr.hostnameInfo.metadata.kind
        const ctx = { packageId, addSsl: iface.addSsl, acme: addr.acme, watch }
        if (kind === 'public-domain' && addr.hostnameInfo.port !== null) {
          await this.checkPublicDomain(
            addr.hostnameInfo.hostname,
            gatewayId,
            addr.hostnameInfo.port,
            addr.count,
            ctx,
          )
        } else if (kind === 'private-domain') {
          await this.checkPrivateDomain(gatewayId, addr.hostnameInfo.hostname)
        } else if (
          kind === 'ipv4' &&
          addr.access === 'public' &&
          addr.hostnameInfo.port !== null &&
          // A port range spans many ports; a single-port reachability check
          // would be misleading, so don't auto-test it on enable.
          addr.count === 1
        ) {
          await this.checkPortForward(gatewayId, addr.hostnameInfo.port, ctx)
        }
      } finally {
        watch.stop()
      }
    }, 'Saving')
  }

  async checkPublicDomain(
    fqdn: string,
    gatewayId: string,
    portOrRes: number | T.AddPublicDomainRes,
    count: number,
    ctx: AddressCheckContext,
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
      let challenge: T.CheckChallengeRes | null

      if (typeof portOrRes === 'number') {
        port = portOrRes
        const [dnsRes, portRes, portV6Res, challengeRes] = await Promise.all([
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
          isRange || !ctx.acme || portOrRes === CHALLENGE_PORT
            ? Promise.resolve(null)
            : this.api
                .checkChallenge({
                  fqdn,
                  gateway: gatewayId,
                  acme: ctx.acme,
                })
                // Null means the domain needs nothing; a failure must not say that.
                .catch(
                  (): T.CheckChallengeRes => ({
                    port: null,
                    portV6: null,
                  }),
                ),
        ])
        dns = dnsRes
        portResult = portRes
        portV6Result = portV6Res
        challenge = challengeRes
      } else {
        dns = portOrRes.dns
        port = portOrRes.port.port
        portResult = isRange ? null : portOrRes.port
        portV6Result = isRange ? null : portOrRes.portV6
        challenge = isRange ? null : portOrRes.challenge
      }

      // A non-SSL binding is served directly by the service, so if the service
      // was down for any part of this request — restarted by the change, or
      // already stopped — nothing was listening and its port/firewall probes say
      // nothing about the user's network. Present those as untested (null →
      // neutral) rather than failed. An SSL binding is fronted by the always-up
      // OS reverse proxy, so its probe stays valid regardless and a failure there
      // is genuine; hence the !addSsl guard. The modal still opens below (a
      // neutralized probe leaves portOk false) so the user learns the port needs
      // forwarding and can retest once the service is running.
      if (!ctx.addSsl && ctx.watch?.wasNonRunning) {
        if (portResult && !portResult.openExternally) portResult = null
        if (portV6Result && !portV6Result.openExternally) portV6Result = null
      }

      const dnsPass = dnsAllPass(dns, gateway.ipInfo.wanIp, gua)
      const portOk = isRange || portAllPass(portResult, portV6Result, gua)
      const challengeOk =
        !challenge || externalAllPass(challenge.port, challenge.portV6, gua)

      if (!dnsPass || !portOk || !challengeOk) {
        setTimeout(
          () =>
            this.openPublicDomainModal({
              fqdn,
              gateway,
              port,
              count,
              packageId: ctx.packageId,
              addSsl: ctx.addSsl,
              acme: ctx.acme ?? null,
              challenge,
              initialResults: { dns, portResult, portV6Result },
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
    count: number,
    ctx: AddressCheckContext,
  ): Promise<void> {
    try {
      const gateway = await this.getGatewayData(gatewayId)
      if (!gateway) return

      this.openPublicDomainModal({
        fqdn,
        gateway,
        port,
        count,
        packageId: ctx.packageId,
        addSsl: ctx.addSsl,
        acme: ctx.acme ?? null,
        // Nothing has probed 443 here.
        challenge: null,
      })
    } catch (e: any) {
      this.errorService.handleError(e)
    }
  }

  async checkPortForward(
    gatewayId: string,
    port: number,
    ctx: AddressCheckContext,
  ): Promise<void> {
    try {
      const gateway = await this.getGatewayData(gatewayId)
      if (!gateway) return

      let portResult = await this.api
        .checkPort({ gateway: gatewayId, port })
        .catch((): null => null)

      // See checkPublicDomain: only a non-SSL binding (served directly by the
      // service) goes unreachable while the service is down, so suppress a
      // spurious failure to neutral only when addSsl is false. The modal still
      // opens (portOk stays false) to surface the forwarding requirement.
      if (
        !ctx.addSsl &&
        ctx.watch?.wasNonRunning &&
        portResult &&
        !portResult.openExternally
      ) {
        portResult = null
      }

      const portOk = !!portResult?.openExternally && !!portResult?.hairpinning

      if (!portOk) {
        setTimeout(
          () =>
            this.openPortForwardModal(
              gateway,
              port,
              1,
              ctx.packageId,
              ctx.addSsl,
              { portResult },
            ),
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
    count: number,
    ctx: AddressCheckContext,
  ): Promise<void> {
    try {
      const gateway = await this.getGatewayData(gatewayId)
      if (!gateway) return

      this.openPortForwardModal(gateway, port, count, ctx.packageId, ctx.addSsl)
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

  private openPublicDomainModal(data: DomainValidationData) {
    this.dialog
      .openComponent(DOMAIN_VALIDATION, {
        label: 'Address Requirements',
        size: 'm',
        data,
      })
      .subscribe()
  }

  private openPortForwardModal(
    gateway: DnsGateway,
    port: number,
    count: number,
    packageId: string,
    addSsl: boolean,
    initialResults?: { portResult: T.CheckPortRes | null },
  ) {
    this.dialog
      .openComponent(PORT_FORWARD_VALIDATION, {
        label: 'Address Requirements',
        size: 'm',
        data: { gateway, port, count, packageId, addSsl, initialResults },
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
