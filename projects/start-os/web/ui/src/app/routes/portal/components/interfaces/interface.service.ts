import { inject, Injectable } from '@angular/core'
import { T, utils } from '@start9labs/start-core'
import { tuiDefaultSort } from '@taiga-ui/cdk'
import { ConfigService } from 'src/app/services/config.service'
import { GatewayPlus } from 'src/app/services/gateway.service'
import { PackageDataEntry } from 'src/app/services/patch-db/data-model'
import {
  PrimaryStatus,
  renderPkgStatus,
} from 'src/app/services/pkg-status-rendering.service'
import { toAuthorityName } from 'src/app/utils/acme'
import { getManifest } from 'src/app/utils/get-package-data'

function isPublicIp(h: T.HostnameInfo): boolean {
  return h.public && (h.metadata.kind === 'ipv4' || h.metadata.kind === 'ipv6')
}

// An IPv6 global-unicast address (GUA) — not loopback / ULA (fc00::/7) /
// link-local (fe80::/10). Mirrors the backend's `ipv6_is_local` complement, so
// the UI offers the Local/Public dropdown for exactly the addresses the
// backend treats as GUAs. A GUA's WAN opt-in lives in its `public` flag
// (projected from `gua_wan`); enable/disable uses the generic rules.
function isGua(h: T.HostnameInfo): boolean {
  if (h.metadata.kind !== 'ipv6') return false
  const first = h.hostname.split(':')[0]
  if (!first) return false // leading "::" (loopback/unspecified) is not a GUA
  const hextet = parseInt(first, 16)
  if (Number.isNaN(hextet)) return false
  const isUla = (hextet & 0xfe00) === 0xfc00
  const isLinkLocal = (hextet & 0xffc0) === 0xfe80
  return !isUla && !isLinkLocal
}

function isEnabled(addr: T.DerivedAddressInfo, h: T.HostnameInfo): boolean {
  if (isPublicIp(h)) {
    if (h.port === null) return true
    const sa =
      h.metadata.kind === 'ipv6'
        ? `[${h.hostname}]:${h.port}`
        : `${h.hostname}:${h.port}`
    return addr.enabled.includes(sa)
  } else {
    return !addr.disabled.some(
      ([hostname, port]) => hostname === h.hostname && port === (h.port ?? 0),
    )
  }
}

function getGatewayIds(h: T.HostnameInfo): string[] {
  switch (h.metadata.kind) {
    case 'ipv4':
    case 'ipv6':
    case 'public-domain':
      return [h.metadata.gateway]
    case 'mdns':
    case 'private-domain':
      return h.metadata.gateways
    case 'plugin':
      return []
  }
}

function getCertificate(
  h: T.HostnameInfo,
  host: T.Host,
  addSsl: T.AddSslOptions | null,
  secure: T.Security | null,
): string {
  if (!h.ssl) return '-'

  // The service holds its own certificate on a binding StartOS does not
  // terminate, whatever authority a domain on this host names.
  if (!addSsl) return secure?.ssl ? 'Self signed' : '-'

  if (h.metadata.kind === 'public-domain') {
    const config = host.publicDomains[h.hostname]
    return config ? toAuthorityName(config.acme) : toAuthorityName(null)
  }

  return toAuthorityName(null)
}

function sortDomainsFirst(a: GatewayAddress, b: GatewayAddress): number {
  const isDomain = (addr: GatewayAddress) =>
    addr.hostnameInfo.metadata.kind === 'public-domain' ||
    (addr.hostnameInfo.metadata.kind === 'private-domain' &&
      !addr.hostnameInfo.hostname.endsWith('.local'))
  return Number(isDomain(b)) - Number(isDomain(a))
}

function getAddressType(h: T.HostnameInfo): string {
  switch (h.metadata.kind) {
    case 'ipv4':
      return 'IPv4'
    case 'ipv6':
      return 'IPv6'
    case 'public-domain':
    case 'private-domain':
      return 'Domain'
    case 'mdns':
      return 'mDNS'
    case 'plugin':
      return 'Plugin'
  }
}

@Injectable({
  providedIn: 'root',
})
export class InterfaceService {
  private readonly config = inject(ConfigService)

  // The package's service interfaces, mapped for display: single-port
  // interfaces first (reached host -> binding -> interface), then port ranges
  // (host -> bindingRange -> interface, carrying a `portRange` span).
  getServiceInterfaces(
    pkg: PackageDataEntry,
    gateways: GatewayPlus[],
    allPackageData?: Record<string, PackageDataEntry>,
  ): MappedServiceInterface[] {
    const { hosts } = pkg

    const all = Object.values(hosts).flatMap(host =>
      Object.values(host.bindings).flatMap(binding =>
        Object.values(binding.interfaces),
      ),
    )

    const single = all
      .sort(tuiDefaultSort)
      .map<MappedServiceInterface>(iFace => {
        const hostId = iFace.addressInfo.hostId || ''
        const host = hosts[hostId]
        const sharedHostNames = all
          .filter(si => si.addressInfo.hostId === hostId && si.id !== iFace.id)
          .map(si => si.name)

        return {
          ...iFace,
          gatewayGroups: host
            ? this.getGatewayGroups(iFace, host, gateways)
            : [],
          pluginGroups: host
            ? this.getPluginGroups(iFace, host, allPackageData)
            : [],
          // A public domain applies to the whole host, so the CA selector must
          // appear whenever any binding on it is SSL — not only this interface's.
          anyAddSsl: Object.values(host?.bindings ?? {}).some(
            b => !!b.options.addSsl,
          ),
          // This interface's own binding: gates whether port/firewall tests are
          // meaningful while the service is stopped (see MappedServiceInterface).
          addSsl:
            !!host?.bindings[iFace.addressInfo.internalPort]?.options.addSsl,
          sharedHostNames,
        }
      })

    const ranges = Object.entries(hosts)
      .flatMap(([hostId, host]) =>
        Object.entries(host.bindingRanges)
          .filter(([, range]) => range.interface)
          .map<MappedServiceInterface>(([key, range]) => {
            const iface = range.interface!
            const internalStartPort = Number(key)
            const end = range.externalStartPort + range.numberOfPorts - 1
            // Representative addressInfo: `internalPort` is the range's internal
            // start port (its key in bindingRanges) so the per-address toggle
            // and add-domain effects target the range; `scheme` prefixes URLs.
            const addressInfo: T.AddressInfo = {
              username: null,
              hostId,
              internalPort: internalStartPort,
              scheme: iface.scheme,
              sslScheme: null,
              suffix: '',
            }

            return {
              id: iface.id,
              name: iface.name,
              description: iface.description,
              masked: false,
              type: 'api',
              addressInfo,
              gatewayGroups: this.getRangeGatewayGroups(
                addressInfo,
                range.addresses,
                host,
                gateways,
                range.numberOfPorts,
              ),
              pluginGroups: [],
              anyAddSsl: Object.values(host.bindings).some(
                b => !!b.options.addSsl,
              ),
              // Port ranges are always non-SSL (bound directly by the service).
              addSsl: false,
              sharedHostNames: [],
              portRange:
                range.numberOfPorts > 1
                  ? `${range.externalStartPort}-${end}`
                  : `${range.externalStartPort}`,
            }
          }),
      )
      .sort((a, b) => a.addressInfo.internalPort - b.addressInfo.internalPort)

    return [...single, ...ranges]
  }

  getGatewayGroups(
    serviceInterface: T.ServiceInterface,
    host: T.Host,
    gateways: GatewayPlus[],
  ): GatewayAddressGroup[] {
    const binding = host.bindings[serviceInterface.addressInfo.internalPort]
    if (!binding) return []

    return this.buildGatewayGroups(
      binding.addresses,
      serviceInterface.addressInfo,
      serviceInterface.masked,
      serviceInterface.type === 'ui',
      binding.options.addSsl,
      binding.options.secure,
      host,
      gateways,
    )
  }

  // Port ranges reuse the same per-gateway address grouping, but their
  // addresses live on the range binding and are always non-SSL (no cert/CA).
  // `numberOfPorts` drives the port-span shown in URLs and forwarding rules.
  getRangeGatewayGroups(
    addressInfo: T.AddressInfo,
    addresses: T.DerivedAddressInfo,
    host: T.Host,
    gateways: GatewayPlus[],
    numberOfPorts: number,
  ): GatewayAddressGroup[] {
    return this.buildGatewayGroups(
      addresses,
      addressInfo,
      false,
      false,
      null,
      null,
      host,
      gateways,
      numberOfPorts,
    )
  }

  private buildGatewayGroups(
    addr: T.DerivedAddressInfo,
    addressInfo: T.AddressInfo,
    masked: boolean,
    ui: boolean,
    addSsl: T.AddSslOptions | null,
    secure: T.Security | null,
    host: T.Host,
    gateways: GatewayPlus[],
    count = 1,
  ): GatewayAddressGroup[] {
    const groupMap = new Map<string, GatewayAddress[]>()
    const gatewayMap = new Map<string, GatewayPlus>()

    for (const gateway of gateways) {
      groupMap.set(gateway.id, [])
      gatewayMap.set(gateway.id, gateway)
    }

    const available =
      this.config.accessType === 'localhost'
        ? addr.available
        : utils.filterNonLocal(addr.available)

    for (const h of available) {
      const gatewayIds = getGatewayIds(h)
      for (const gid of gatewayIds) {
        const list = groupMap.get(gid)
        if (!list) continue
        list.push({
          enabled: isEnabled(addr, h),
          gua: isGua(h),
          type: getAddressType(h),
          access: h.public ? 'public' : 'private',
          // A port range exposes a span of ports, not one, so its URL is the
          // bare host (no port) — the span is shown once in the interface
          // header, never faked onto a per-row URL that no one would copy.
          url:
            count > 1
              ? utils.addressHostToUrl(addressInfo, { ...h, port: null })
              : utils.addressHostToUrl(addressInfo, h),
          hostnameInfo: h,
          masked,
          ui,
          deletable:
            h.metadata.kind === 'private-domain' ||
            h.metadata.kind === 'public-domain',
          certificate: getCertificate(h, host, addSsl, secure),
          count,
        })
      }
    }

    return gateways
      .filter(g => (groupMap.get(g.id)?.length ?? 0) > 0)
      .map(g => {
        const addresses = groupMap.get(g.id)!.sort(sortDomainsFirst)

        // mDNS resolves only via enabled LAN IPs on this gateway
        const enabledHostnames = addresses
          .filter(a => a.enabled)
          .map(a => a.hostnameInfo)
        for (const a of addresses) {
          if (a.hostnameInfo.metadata.kind === 'mdns') {
            a.enabled = utils.mdnsResolvable(a.hostnameInfo, enabledHostnames)
          }
        }

        return {
          gatewayId: g.id,
          gatewayName: g.name,
          deviceType: g.ipInfo?.deviceType || 'ethernet',
          addresses,
        }
      })
  }

  getPluginGroups(
    serviceInterface: T.ServiceInterface,
    host: T.Host,
    allPackageData?: Record<string, T.PackageDataEntry>,
  ): PluginAddressGroup[] {
    const binding = host.bindings[serviceInterface.addressInfo.internalPort]
    if (!binding) return []

    const addr = binding.addresses
    const masked = serviceInterface.masked
    const groupMap = new Map<string, PluginAddress[]>()

    const { scheme, sslScheme } = serviceInterface.addressInfo

    for (const h of addr.available) {
      if (h.metadata.kind !== 'plugin') continue

      const url = utils.addressHostToUrl(serviceInterface.addressInfo, h)
      const pluginId = h.metadata.packageId

      if (!groupMap.has(pluginId)) {
        groupMap.set(pluginId, [])
      }

      groupMap.get(pluginId)!.push({
        url,
        scheme: h.ssl ? sslScheme : scheme,
        hostnameInfo: h,
        masked,
      })
    }

    // Also include URL plugins that have no addresses yet
    if (allPackageData) {
      for (const [pkgId, pkg] of Object.entries(allPackageData)) {
        if (pkg.plugin?.url && !groupMap.has(pkgId)) {
          groupMap.set(pkgId, [])
        }
      }
    }

    return Array.from(groupMap.entries()).map(([pluginId, addresses]) => {
      const pluginPkg = allPackageData?.[pluginId]
      const pluginActions = pluginPkg?.actions ?? {}
      const tableActionId = pluginPkg?.plugin?.url?.tableAction ?? null
      const tableActionMeta = tableActionId
        ? pluginActions[tableActionId]
        : undefined
      const tableAction =
        tableActionId && tableActionMeta
          ? { id: tableActionId, metadata: tableActionMeta }
          : null

      let pluginPkgInfo: PluginPkgInfo | null = null
      if (pluginPkg) {
        const manifest = getManifest(pluginPkg)
        pluginPkgInfo = {
          id: manifest.id,
          title: manifest.title,
          icon: pluginPkg.icon,
          status: renderPkgStatus(pluginPkg).primary,
        }
      }

      return {
        pluginId,
        pluginName:
          pluginPkgInfo?.title ??
          pluginId.charAt(0).toUpperCase() + pluginId.slice(1),
        addresses,
        tableAction,
        pluginPkgInfo,
        pluginActions,
      }
    })
  }

  launchableAddress(ui: T.ServiceInterface, host: T.Host): string {
    const addresses = utils.filledAddress(host, ui.addressInfo)

    if (!addresses.hostnames.length) return ''

    const publicDomains = addresses.filter({
      kind: 'domain',
      visibility: 'public',
    })
    const wanIp = addresses.filter({ kind: 'ipv4', visibility: 'public' })
    const bestPublic = [publicDomains, wanIp].flatMap(h =>
      h.format('urlstring'),
    )[0]
    const privateDomains = addresses.filter({
      kind: 'domain',
      visibility: 'private',
    })
    const mdns = addresses.filter({ kind: 'mdns' })
    const bestPrivate = [privateDomains, mdns].flatMap(h =>
      h.format('urlstring'),
    )[0]

    let matching
    let onLan = false
    switch (this.config.accessType) {
      case 'ipv4':
        matching = addresses.nonLocal
          .filter({
            kind: 'ipv4',
            predicate: h => h.hostname === this.config.hostname,
          })
          .format('urlstring')[0]
        onLan = true
        break
      case 'ipv6':
        matching = addresses.nonLocal
          .filter({
            kind: 'ipv6',
            predicate: h => h.hostname === this.config.hostname,
          })
          .format('urlstring')[0]
        break
      case 'localhost':
        matching = addresses
          .filter({ kind: 'localhost' })
          .format('urlstring')[0]
        onLan = true
        break
      case 'mdns':
        matching = mdns.format('urlstring')[0]
        onLan = true
        break
      case 'domain':
        matching = publicDomains.format('urlstring')[0]
        break
      case 'tor':
        matching = addresses
          .filter({
            pluginId: 'tor',
          })
          .format('urlstring')[0]
        break
      case 'wan-ipv4':
        matching = wanIp.format('urlstring')[0]
        break
    }

    if (matching) return matching
    if (onLan && bestPrivate) return bestPrivate
    if (bestPublic) return bestPublic
    if (bestPrivate) return bestPrivate
    return ''
  }
}

export type GatewayAddress = {
  enabled: boolean
  // An IPv6 GUA gets a Local/Public dropdown in the access column (its WAN
  // opt-in, carried by `hostnameInfo.public`); other addresses are read-only.
  gua: boolean
  type: string
  access: 'public' | 'private'
  url: string
  hostnameInfo: T.HostnameInfo
  masked: boolean
  ui: boolean
  deletable: boolean
  certificate: string
  // Number of forwarded ports: 1 for a single-port binding, the range span for
  // a port range. Drives the port-span shown in forwarding rules.
  count: number
}

export type GatewayAddressGroup = {
  gatewayId: string
  gatewayName: string
  deviceType: string
  addresses: GatewayAddress[]
}

export type PluginAddress = {
  url: string
  scheme: string | null
  hostnameInfo: T.HostnameInfo
  masked: boolean
}

export type PluginPkgInfo = {
  id: string
  title: string
  icon: string
  status: PrimaryStatus
}

export type PluginAddressGroup = {
  pluginId: string
  pluginName: string
  addresses: PluginAddress[]
  tableAction: { id: string; metadata: T.ActionMetadata } | null
  pluginPkgInfo: PluginPkgInfo | null
  pluginActions: Record<string, T.ActionMetadata>
}

export type MappedServiceInterface = T.ServiceInterface & {
  gatewayGroups: GatewayAddressGroup[]
  pluginGroups: PluginAddressGroup[]
  // True when any binding on this interface's host adds SSL, not just this
  // interface's own binding. A public domain is stored on the host and applies
  // to every binding on it, so the add-domain form must offer a Certificate
  // Authority whenever any of those bindings is SSL.
  anyAddSsl: boolean
  // Whether this interface's own binding is SSL-terminated by the OS reverse
  // proxy. When true the OS (always up) owns the external port, so port-forward
  // and firewall reachability can be tested even while the service is stopped;
  // when false the service binds the port directly, so those tests are only
  // meaningful while it is running.
  addSsl: boolean
  sharedHostNames: string[]
  // Present only for a port-range interface: the external port span shown as a
  // badge (e.g. "8000-8010").
  portRange?: string
}

// The tuiBadge appearance for a service interface's type, shared by the
// interfaces list and the interface detail header.
export function getInterfaceBadgeAppearance(
  type: T.ServiceInterfaceType = 'ui',
): string {
  switch (type) {
    case 'ui':
      return 'positive'
    case 'api':
      return 'info'
    case 'p2p':
      return 'negative'
  }
}
