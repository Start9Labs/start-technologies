import { computed, inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { T, utils } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { DataModel } from './patch-db/data-model'

export type GatewayPlus = T.NetworkInterfaceInfo & {
  id: string
  name: string
  ipInfo: T.IpInfo
  subnets: utils.IpNet[]
  lanIpv4: string[]
  wanIp?: utils.IpAddress
}

@Injectable()
export class GatewayService {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  private readonly network = toSignal(
    this.patch.watch$('serverInfo', 'network'),
  )

  readonly defaultOutbound = computed(() => this.network()?.defaultOutbound)

  readonly defaultOutboundGateway = computed(() => {
    const network = this.network()
    const id = network?.defaultOutbound
    if (!id) return null
    const gateway = network.gateways[id]
    return {
      id,
      name: gateway?.name ?? gateway?.ipInfo?.name ?? id,
    }
  })

  readonly gateways = computed(() => {
    const network = this.network()
    if (!network) return
    return Object.entries(network.gateways)
      .filter(([_, val]) => !!val?.ipInfo)
      .filter(
        ([_, val]) =>
          val?.ipInfo?.deviceType !== 'bridge' &&
          val?.ipInfo?.deviceType !== 'loopback',
      )
      .map(([id, val]) => {
        const subnets = val.ipInfo?.subnets.map(s => utils.IpNet.parse(s)) ?? []
        const name = val.name ?? val.ipInfo!.name
        return {
          ...val,
          id,
          name,
          subnets,
          lanIpv4: subnets.filter(s => s.isIpv4()).map(s => s.address),
          wanIp: val.ipInfo?.wanIp && utils.IpAddress.parse(val.ipInfo?.wanIp),
        } as GatewayPlus
      })
  })
}
