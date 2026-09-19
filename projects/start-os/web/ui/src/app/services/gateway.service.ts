import { inject, Injectable } from '@angular/core'
import { PatchDB } from 'patch-db-client'
import { T, utils } from '@start9labs/start-core'
import { map } from 'rxjs'
import { getWanIp } from '../utils/gua'
import { DataModel } from './patch-db/data-model'
import { toSignal } from '@angular/core/rxjs-interop'

export type GatewayPlus = T.NetworkInterfaceInfo & {
  id: string
  name: string
  ipInfo: T.IpInfo
  subnets: utils.IpNet[]
  lanIpv4: string[]
  effectiveWanIp: string | null
}

@Injectable()
export class GatewayService {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  private readonly network$ = this.patch.watch$('serverInfo', 'network')

  readonly defaultOutbound = toSignal(
    this.network$.pipe(map(n => n.defaultOutbound)),
  )

  readonly gateways = toSignal(
    this.network$.pipe(
      map(network => {
        const gateways = network.gateways
        return Object.entries(gateways)
          .filter(([_, val]) => !!val?.ipInfo)
          .filter(
            ([_, val]) =>
              val?.ipInfo?.deviceType !== 'bridge' &&
              val?.ipInfo?.deviceType !== 'loopback',
          )
          .map(([id, val]) => {
            const subnets =
              val.ipInfo?.subnets.map(s => utils.IpNet.parse(s)) ?? []
            const name = val.name ?? val.ipInfo!.name
            return {
              ...val,
              id,
              name,
              subnets,
              lanIpv4: subnets.filter(s => s.isIpv4()).map(s => s.address),
              effectiveWanIp: getWanIp(val),
            } as GatewayPlus
          })
      }),
    ),
  )
}
