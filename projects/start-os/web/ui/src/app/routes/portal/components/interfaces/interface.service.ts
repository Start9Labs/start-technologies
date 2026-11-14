import { inject, Injectable } from '@angular/core'
import { T, utils } from '@start9labs/start-core'
import { selectLaunchableAddress } from '@start9labs/start-core/util/selectLaunchableAddress'
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

// A public IPv6 row is a GUA, which the name resolves to like any LAN IP.
function servesMdns(h: T.HostnameInfo): boolean {
  return h.metadata.kind === 'ipv6' || (h.metadata.kind === 'ipv4' && !h.public)
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

  return toAuthorityName(getAcmeProvider(h, host, addSsl))
}

// Null unless StartOS terminates this address's TLS and issues its certificate.
function getAcmeProvider(
  h: T.HostnameInfo,
  host: T.Host,
  addSsl: T.AddSslOptions | null,
): T.AcmeProvider | null {
  if (!h.ssl || !addSsl || h.metadata.kind !== 'public-domain') return null
  return host.publicDomains[h.hostname]?.acme ?? null
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

  getServiceInterfaces(
    pkg: Pick<PackageDataEntry, 'hosts'>,
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
      .map(iFace =>
        this.mapServiceInterface(
          iFace,
          hosts[iFace.addressInfo.hostId || ''],
          gateways,
          allPackageData,
        ),
      )

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
              defaultOff: false,
              pendingPort: false,
              name: iface.name,
              description: iface.description,
              masked: false,
              type: 'api',
              preferredLauncherAddress: null,
              addressInfo,
              gatewayGroups: this.getRangeGatewayGroups(
                addressInfo,
                range,
                host,
                gateways,
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

  mapServiceInterface(
    iface: T.ServiceInterface,
    host: T.Host | undefined,
    gateways: GatewayPlus[],
    allPackageData?: Record<string, PackageDataEntry>,
  ): MappedServiceInterface {
    const bindings = Object.values(host?.bindings ?? {})
    const binding = host?.bindings[iface.addressInfo.internalPort]
    const defaultOff = binding?.options.defaultOff ?? false
    return {
      ...iface,
      defaultOff,
      pendingPort: false,
      gatewayGroups: host ? this.getGatewayGroups(iface, host, gateways) : [],
      pluginGroups: host
        ? this.getPluginGroups(iface, host, allPackageData).filter(
            group => !defaultOff || group.addresses.length > 0,
          )
        : [],
      anyAddSsl: bindings.some(binding => !!binding.options.addSsl),
      addSsl: !!binding?.options.addSsl,
      sharedHostNames: bindings
        .flatMap(binding => Object.values(binding.interfaces))
        .filter(other => other.id !== iface.id)
        .map(other => other.name),
    }
  }

  mapPreviewServiceInterface(
    iface: T.ServiceInterface,
    host: T.Host,
    preview: T.BindingPreview,
    gateways: GatewayPlus[],
    allPackageData?: Record<string, PackageDataEntry>,
  ): MappedServiceInterface {
    return {
      ...this.mapServiceInterface(iface, host, gateways, allPackageData),
      defaultOff: preview.options.defaultOff,
      pendingPort: true,
      gatewayGroups: this.buildGatewayGroups(
        preview.addresses,
        iface.addressInfo,
        iface.masked,
        iface.type === 'ui',
        preview.options,
        host,
        gateways,
      ),
      pluginGroups: [],
    }
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
      binding.options,
      host,
      gateways,
    )
  }

  private getRangeGatewayGroups(
    addressInfo: T.AddressInfo,
    range: T.RangeBindInfo,
    host: T.Host,
    gateways: GatewayPlus[],
  ): GatewayAddressGroup[] {
    return this.buildGatewayGroups(
      range.addresses,
      addressInfo,
      false,
      false,
      { transport: range.transport, proxy: null, addSsl: null, secure: null },
      host,
      gateways,
      range.numberOfPorts,
    )
  }

  private buildGatewayGroups(
    addr: T.DerivedAddressInfo,
    addressInfo: T.AddressInfo,
    masked: boolean,
    ui: boolean,
    options: Pick<T.BindOptions, 'transport' | 'proxy' | 'addSsl' | 'secure'>,
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
      const enabled = utils.isAddressEnabled(addr, h)
      if (utils.isAddressInsecure(addr, h) && !enabled) continue
      const gatewayIds = getGatewayIds(h)
      for (const gid of gatewayIds) {
        const list = groupMap.get(gid)
        if (!list) continue
        list.push({
          enabled,
          allIpsDisabled: false,
          gua: isGua(h) && (options.secure !== null || h.ssl),
          type: getAddressType(h),
          transport: utils.addressTransport(options, h),
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
          certificate: getCertificate(h, host, options.addSsl, options.secure),
          acme: getAcmeProvider(h, host, options.addSsl),
          count,
        })
      }
    }

    return gateways
      .filter(g => (groupMap.get(g.id)?.length ?? 0) > 0)
      .map(g => {
        const addresses = groupMap.get(g.id)!.sort(sortDomainsFirst)

        for (const mdns of addresses) {
          if (
            mdns.enabled &&
            !mdns.hostnameInfo.ssl &&
            mdns.hostnameInfo.metadata.kind === 'mdns'
          ) {
            const ips = addresses.filter(
              a =>
                a.hostnameInfo.port === mdns.hostnameInfo.port &&
                servesMdns(a.hostnameInfo),
            )
            mdns.allIpsDisabled = !!ips.length && ips.every(a => !a.enabled)
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
    return selectLaunchableAddress(ui, host, {
      accessType: this.config.accessType,
      hostname: this.config.hostname,
    })
  }
}

export type GatewayAddress = {
  enabled: boolean
  // An enabled non-SSL mDNS address whose gateway has IPs, none of them enabled.
  allIpsDisabled: boolean
  // An IPv6 GUA gets a Local/Public dropdown in the access column (its WAN
  // opt-in, carried by `hostnameInfo.public`); other addresses are read-only.
  gua: boolean
  type: string
  transport: T.Transport
  access: 'public' | 'private'
  url: string
  hostnameInfo: T.HostnameInfo
  masked: boolean
  ui: boolean
  deletable: boolean
  certificate: string
  // The address's ACME authority. Such an address also needs 443 reachable.
  acme: T.AcmeProvider | null
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
  defaultOff: boolean
  pendingPort: boolean
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
