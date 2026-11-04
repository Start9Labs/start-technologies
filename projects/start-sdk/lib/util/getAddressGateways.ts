import {
  Effects,
  GatewayId,
  HostId,
  HostnameInfo,
} from '@start9labs/start-core/types'
import { isAddressEnabled } from '@start9labs/start-core/util/filledAddress'
import { getOwnHost } from '@start9labs/start-core/util/GetHostInfo'
import { GetOutboundGateway } from '@start9labs/start-core/util/GetOutboundGateway'
import { IpAddress } from '@start9labs/start-core/util/ip'

/** Result of {@link getAddressGateways}. */
export type AddressGateways = {
  /** The gateway this service's outbound connections leave through. */
  outbound: GatewayId
  /** Each address passed in, with the gateway publishing it, or `null` when no gateway publishes it. */
  addresses: { address: string; gateway: GatewayId | null }[]
}

type Published = HostnameInfo & { metadata: { gateway: GatewayId } }

const matches = (published: Published, address: string) => {
  if (published.metadata.kind !== 'ipv4' && published.metadata.kind !== 'ipv6')
    return published.hostname.toLowerCase() === address.toLowerCase()
  try {
    return IpAddress.parse(published.hostname).cmp(address) === 0
  } catch {
    return false
  }
}

/**
 * Resolve the gateways publishing a service's advertised addresses, beside the
 * gateway its outbound connections leave through.
 *
 * Pass the addresses the service actually advertises or authorizes, read back
 * from its rendered configuration. Equal gateways establish gateway
 * consistency only: outbound traffic may still carry a different source
 * address of the same gateway. Each call reads both values once, so a health
 * check may call it on every poll.
 *
 * @param effects - The effects context
 * @param hostId - The host whose bindings publish the addresses
 * @param addresses - IP literals or domains the service advertises
 */
export async function getAddressGateways(
  effects: Effects,
  hostId: HostId,
  addresses: string[],
): Promise<AddressGateways> {
  const [outbound, host] = await Promise.all([
    new GetOutboundGateway(effects).once(),
    getOwnHost(effects, hostId).once(),
  ])
  const published = host
    ? [
        ...Object.values(host.bindings)
          .filter(b => b.enabled && Object.keys(b.interfaces).length > 0)
          .map(b => b.addresses),
        ...Object.values(host.bindingRanges)
          .filter(r => r.enabled && r.interface !== null)
          .map(r => r.addresses),
      ].flatMap(addrs =>
        addrs.available.filter(
          (h): h is Published =>
            h.public && 'gateway' in h.metadata && isAddressEnabled(addrs, h),
        ),
      )
    : []
  return {
    outbound,
    addresses: addresses.map(address => ({
      address,
      gateway:
        published.find(h => matches(h, address))?.metadata.gateway ?? null,
    })),
  }
}
