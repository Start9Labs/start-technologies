import { computed, inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { T, utils } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'

import { DataModel } from './patch-db/data-model'

type ConnectedGateway = T.NetworkInterfaceInfo & { ipInfo: T.IpInfo }

export function isConnectedGateway(
  gateway: T.NetworkInterfaceInfo | undefined,
): gateway is ConnectedGateway {
  return (
    !!gateway?.ipInfo &&
    !['bridge', 'loopback'].includes(gateway.ipInfo.deviceType ?? '')
  )
}

export function outboundGatewayOptions(
  gateways: Record<string, T.NetworkInterfaceInfo>,
  selected: string | null,
): Record<string, string> {
  const ids = Object.entries(gateways)
    .filter(([id, gateway]) => id === selected || isConnectedGateway(gateway))
    .map(([id]) => id)
  if (selected && !ids.includes(selected)) ids.push(selected)
  return Object.fromEntries(
    ids.map(id => [id, `${gatewayName(id, gateways[id])} (${id})`]),
  )
}

export type GatewayPlus = ConnectedGateway & {
  id: string
  name: string
  subnets: utils.IpNet[]
  lanIpv4: string[]
  wanIp?: utils.IpAddress
}

export function gatewayName(
  id: string,
  gateway?: T.NetworkInterfaceInfo,
): string {
  return gateway?.name ?? gateway?.ipInfo?.name ?? id
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
    return { id, name: gatewayName(id, network.gateways[id]) }
  })

  readonly gateways = computed(() => {
    const network = this.network()
    if (!network) return
    return Object.entries(network.gateways).flatMap(
      ([id, gateway]): GatewayPlus[] => {
        if (!isConnectedGateway(gateway)) return []
        const subnets = gateway.ipInfo.subnets.map(s => utils.IpNet.parse(s))
        return [
          {
            ...gateway,
            id,
            name: gatewayName(id, gateway),
            subnets,
            lanIpv4: subnets.filter(s => s.isIpv4()).map(s => s.address),
            wanIp: gateway.ipInfo.wanIp
              ? utils.IpAddress.parse(gateway.ipInfo.wanIp)
              : undefined,
          },
        ]
      },
    )
  })
}
