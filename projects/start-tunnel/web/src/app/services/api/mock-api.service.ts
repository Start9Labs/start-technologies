import { inject, Injectable } from '@angular/core'
import { shareReplay, Subject, tap } from 'rxjs'
import { WebSocketSubject } from 'rxjs/webSocket'
import { Api, ApiClient, ApiService } from './api.service'
import { pauseFor } from '@start9labs/shared'
import {
  createRpcClient,
  RpcHandlers,
  T,
  tableTransport,
} from '@start9labs/start-core'
import { AuthService } from '../auth.service'
import {
  AddOperation,
  Operation,
  PatchOp,
  RemoveOperation,
  ReplaceOperation,
  Revision,
} from 'patch-db-client'
import { toObservable } from '@angular/core/rxjs-interop'
import { mockTunnelData } from '../patch-db/data-model'

@Injectable({
  providedIn: 'root',
})
export class MockApiService extends ApiService {
  private readonly auth = inject(AuthService)
  readonly mockWsSource$ = new Subject<Revision>()
  sequence = 1
  private readonly handlers: RpcHandlers<Api> = {
    'db.subscribe': async () => {
      await pauseFor(1000)
      return {
        dump: { id: 1, value: mockTunnelData },
        guid: 'patch-db-guid',
      }
    },
    'auth.login': async params => {
      await pauseFor(1000)
      return null
    },
    'auth.logout': async () => {
      await pauseFor(1000)
      return null
    },
    'auth.set-password': async params => {
      await pauseFor(1000)
      return null
    },
    'subnet.set-dns': async params => {
      await pauseFor(1000)

      const dns: T.Tunnel.DnsConfig =
        params.mode === 'device'
          ? { type: 'device', ip: params.deviceIp! }
          : params.mode === 'custom'
            ? { type: 'custom', servers: params.servers }
            : { type: 'default' }

      const patch: ReplaceOperation<T.Tunnel.DnsConfig>[] = [
        {
          op: PatchOp.REPLACE,
          path: `/wg/subnets/${replaceSlashes(params.subnet)}/dns`,
          value: dns,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'subnet.set-wan': async params => {
      await pauseFor(1000)

      const patch: ReplaceOperation<string | null>[] = [
        {
          op: PatchOp.REPLACE,
          path: `/wg/subnets/${replaceSlashes(params.subnet)}/wanIp`,
          value: params.wanIp ?? null,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'subnet.remove': async params => {
      await pauseFor(1000)

      const patch: RemoveOperation[] = [
        {
          op: PatchOp.REMOVE,
          path: `/wg/subnets/${replaceSlashes(params.subnet)}`,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'device.remove': async params => {
      await pauseFor(1000)

      const patch: RemoveOperation[] = [
        {
          op: PatchOp.REMOVE,
          path: `/wg/subnets/${replaceSlashes(params.subnet)}/clients/${params.ip}`,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'device.show-config': async params => {
      await pauseFor(1000)

      const endpoint = params.endpoint ?? '203.0.113.42'
      const host =
        endpoint.includes(':') && !endpoint.startsWith('[')
          ? `[${endpoint}]`
          : endpoint
      return MOCK_CONFIG.replace('203.0.113.42:51820', `${host}:51820`)
    },
    'device.set-dns-injection': async params => {
      await pauseFor(1000)

      const patch: ReplaceOperation<boolean>[] = [
        {
          op: PatchOp.REPLACE,
          path: `/wg/subnets/${replaceSlashes(params.subnet)}/clients/${params.ip}/allowDnsInjection`,
          value: params.enabled,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'device.set-auto-port-forward': async params => {
      await pauseFor(1000)

      const patch: ReplaceOperation<boolean>[] = [
        {
          op: PatchOp.REPLACE,
          path: `/wg/subnets/${replaceSlashes(params.subnet)}/clients/${params.ip}/allowAutoPortForward`,
          value: params.enabled,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'device.set-wan': async params => {
      await pauseFor(1000)

      const patch: ReplaceOperation<string | null>[] = [
        {
          op: PatchOp.REPLACE,
          path: `/wg/subnets/${replaceSlashes(params.subnet)}/clients/${params.ip}/wanIp`,
          value: params.wanIp ?? null,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'device.set-kind': async params => {
      await pauseFor(1000)

      const base = `/wg/subnets/${replaceSlashes(params.subnet)}/clients/${params.ip}`
      const auto = params.kind === 'server'
      const patch: ReplaceOperation<string | boolean>[] = [
        { op: PatchOp.REPLACE, path: `${base}/kind`, value: params.kind },
        { op: PatchOp.REPLACE, path: `${base}/allowDnsInjection`, value: auto },
        {
          op: PatchOp.REPLACE,
          path: `${base}/allowAutoPortForward`,
          value: auto,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'dns.add': async params => {
      await pauseFor(1000)

      const patch: AddOperation<T.Tunnel.DnsRecordEntry>[] = [
        {
          op: PatchOp.ADD,
          path: `/dnsRecords/-`,
          value: {
            name: params.name,
            type: params.type,
            value: params.value,
            ttl: params.ttl ?? 300,
            source: null,
          },
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'dns.remove': async params => {
      await pauseFor(1000)

      const index = mockTunnelData.dnsRecords.findIndex(
        r => r.name === params.name && (!params.type || r.type === params.type),
      )
      if (index >= 0) {
        const patch: RemoveOperation[] = [
          { op: PatchOp.REMOVE, path: `/dnsRecords/${index}` },
        ]
        this.mockRevision(patch)
      }

      return null
    },
    'port-forward.add': async params => {
      await pauseFor(1000)

      // The external IP is fixed server-side to the target device's WAN.
      const source = `${this.deviceWan(params.target)}:${params.externalPort}`
      const forwards = mockTunnelData.portForwards
      const existing = forwards[source]

      if (params.sni?.length) {
        const routes: { [hostname: string]: T.Tunnel.SniRoute } =
          existing?.kind === 'sni' ? { ...existing.routes } : {}

        for (const hostname of params.sni ?? []) {
          routes[hostname] = {
            target: params.target,
            label: params.label || null,
            enabled: true,
            auto: false,
          }
        }

        const value: T.Tunnel.PortForward = {
          kind: 'sni',
          routes,
          fallback: null,
        }
        forwards[source] = value
        this.mockRevision([
          {
            op: existing ? PatchOp.REPLACE : PatchOp.ADD,
            path: `/portForwards/${source}`,
            value,
          },
        ])
      } else {
        const value: T.Tunnel.PortForward = {
          kind: 'dnat',
          target: params.target,
          label: params.label || null,
          enabled: true,
          count: params.count ?? 1,
          auto: false,
        }
        forwards[source] = value
        this.mockRevision([
          {
            op: existing ? PatchOp.REPLACE : PatchOp.ADD,
            path: `/portForwards/${source}`,
            value,
          },
        ])
      }

      return null
    },
    'port-forward.update-label': async params => {
      await pauseFor(1000)

      const entry = mockTunnelData.portForwards[params.source]
      if (!entry) return null

      if (params.hostname && entry.kind === 'sni') {
        const route = entry.routes[params.hostname]
        if (route) route.label = params.label ?? null
        this.mockRevision([
          {
            op: PatchOp.REPLACE,
            path: `/portForwards/${params.source}/routes/${params.hostname}/label`,
            value: params.label,
          },
        ])
      } else if (entry.kind === 'dnat') {
        entry.label = params.label ?? null
        this.mockRevision([
          {
            op: PatchOp.REPLACE,
            path: `/portForwards/${params.source}/label`,
            value: params.label,
          },
        ])
      }

      return null
    },
    'port-forward.set-enabled': async params => {
      await pauseFor(1000)

      const entry = mockTunnelData.portForwards[params.source]
      if (!entry) return null

      if (params.hostname && entry.kind === 'sni') {
        const route = entry.routes[params.hostname]
        if (route) route.enabled = params.enabled
        this.mockRevision([
          {
            op: PatchOp.REPLACE,
            path: `/portForwards/${params.source}/routes/${params.hostname}/enabled`,
            value: params.enabled,
          },
        ])
      } else if (entry.kind === 'dnat') {
        entry.enabled = params.enabled
        this.mockRevision([
          {
            op: PatchOp.REPLACE,
            path: `/portForwards/${params.source}/enabled`,
            value: params.enabled,
          },
        ])
      }

      return null
    },
    'port-forward.remove': async params => {
      await pauseFor(1000)

      const entry = mockTunnelData.portForwards[params.source]
      if (!entry) return null

      if (params.hostname && entry.kind === 'sni') {
        delete entry.routes[params.hostname]

        if (Object.keys(entry.routes).length) {
          this.mockRevision([
            {
              op: PatchOp.REMOVE,
              path: `/portForwards/${params.source}/routes/${params.hostname}`,
            },
          ])
        } else {
          delete mockTunnelData.portForwards[params.source]
          this.mockRevision([
            { op: PatchOp.REMOVE, path: `/portForwards/${params.source}` },
          ])
        }
      } else {
        delete mockTunnelData.portForwards[params.source]
        this.mockRevision([
          { op: PatchOp.REMOVE, path: `/portForwards/${params.source}` },
        ])
      }

      return null
    },
    'pinhole.add': async params => {
      await pauseFor(1000)

      const key = `[${params.gua}]:${params.externalPort}`
      const existing = mockTunnelData.pinholes6[key]
      const value: T.Tunnel.Pinhole = {
        label: params.label || null,
        enabled: existing?.enabled ?? true,
        count: params.count ?? 1,
        internalPort: params.internalPort ?? null,
        auto: false,
      }
      mockTunnelData.pinholes6[key] = value
      this.mockRevision([
        {
          op: existing ? PatchOp.REPLACE : PatchOp.ADD,
          path: `/pinholes6/${key}`,
          value,
        },
      ])

      return null
    },
    'pinhole.update-label': async params => {
      await pauseFor(1000)

      const key = `[${params.gua}]:${params.externalPort}`
      const entry = mockTunnelData.pinholes6[key]
      if (!entry) return null

      entry.label = params.label ?? null
      this.mockRevision([
        {
          op: PatchOp.REPLACE,
          path: `/pinholes6/${key}/label`,
          value: params.label,
        },
      ])

      return null
    },
    'pinhole.set-enabled': async params => {
      await pauseFor(1000)

      const key = `[${params.gua}]:${params.externalPort}`
      const entry = mockTunnelData.pinholes6[key]
      if (!entry) return null

      entry.enabled = params.enabled
      this.mockRevision([
        {
          op: PatchOp.REPLACE,
          path: `/pinholes6/${key}/enabled`,
          value: params.enabled,
        },
      ])

      return null
    },
    'pinhole.remove': async params => {
      await pauseFor(1000)

      const key = `[${params.gua}]:${params.externalPort}`
      if (!mockTunnelData.pinholes6[key]) return null

      delete mockTunnelData.pinholes6[key]
      this.mockRevision([{ op: PatchOp.REMOVE, path: `/pinholes6/${key}` }])

      return null
    },
    'http-redirect.set-enabled': async params => {
      await pauseFor(1000)

      const disabled = new Set(mockTunnelData.httpRedirects.disabled)
      if (params.enabled) {
        disabled.delete(params.ip)
      } else {
        disabled.add(params.ip)
      }
      mockTunnelData.httpRedirects.disabled = [...disabled]

      const patch: ReplaceOperation<string[]>[] = [
        {
          op: PatchOp.REPLACE,
          path: `/httpRedirects/disabled`,
          value: mockTunnelData.httpRedirects.disabled,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'subnet.set-ipv6': async params => {
      await pauseFor(1000)

      const patch: ReplaceOperation<string | null>[] = [
        {
          op: PatchOp.REPLACE,
          path: `/wg/subnets/${replaceSlashes(params.subnet)}/ipv6`,
          value: params.prefix ?? null,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    restart: async () => {
      await pauseFor(1000)
      return null
    },
    'update.check': async () => {
      await pauseFor(1000)
      return {
        status: 'update-available',
        installed: '1.5.0',
        candidate: '1.5.1',
      }
    },
    'update.apply': async () => {
      await pauseFor(2000)
      return {
        status: 'updating',
        installed: '1.5.0',
        candidate: '1.5.1',
      }
    },
    'subnet.add': async params =>
      params.subnet in mockTunnelData.wg.subnets
        ? this.editSubnet(params)
        : this.addSubnet(params),
    'device.add': async params =>
      params.ip != null &&
      params.ip in (mockTunnelData.wg.subnets[params.subnet]?.clients ?? {})
        ? this.editDevice(params as T.Tunnel.AddDeviceParams)
        : this.addDevice(params as T.Tunnel.AddDeviceParams),
  }
  readonly rpc = createRpcClient<ApiClient>(tableTransport<Api>(this.handlers))

  constructor() {
    super()
    toObservable(this.auth.authenticated)
      .pipe(
        tap(() => {
          this.sequence = 1
        }),
      )
      .subscribe()
  }

  openWebsocket$<T>(guid: string): WebSocketSubject<T> {
    return this.mockWsSource$.pipe(
      shareReplay({ bufferSize: 1, refCount: true }),
    ) as WebSocketSubject<T>
  }

  /** The WAN IP a device's traffic egresses (device override > subnet > default). */
  private deviceWan(target: string): string {
    const ip = target.split(':')[0] ?? ''
    for (const subnet of Object.values(mockTunnelData.wg.subnets)) {
      const client = subnet.clients[ip]
      if (client) return client.wanIp ?? subnet.wanIp ?? this.defaultWan()
    }
    return this.defaultWan()
  }

  private defaultWan(): string {
    const gw = Object.values(mockTunnelData.gateways)[0]
    return gw?.ipInfo?.subnets[0]?.split('/')[0] ?? '0.0.0.0'
  }

  private async mockRevision<T>(patch: Operation<T>[]): Promise<void> {
    const revision = {
      id: ++this.sequence,
      patch,
    }
    this.mockWsSource$.next(revision)
  }
  private async addSubnet(
    params: T.Tunnel.SubnetParams & T.Tunnel.AddSubnetParams,
  ) {
    await pauseFor(1000)

    const patch: AddOperation<T.Tunnel.WgSubnetConfig>[] = [
      {
        op: PatchOp.ADD,
        path: `/wg/subnets/${replaceSlashes(params.subnet)}`,
        value: {
          name: params.name,
          clients: {},
          dns: { type: 'default' },
          wanIp: null,
          ipv6: null,
        },
      },
    ]
    this.mockRevision(patch)

    return null
  }
  private async editSubnet(
    params: T.Tunnel.SubnetParams & T.Tunnel.AddSubnetParams,
  ) {
    await pauseFor(1000)

    const patch: ReplaceOperation<string>[] = [
      {
        op: PatchOp.REPLACE,
        path: `/wg/subnets/${replaceSlashes(params.subnet)}/name`,
        value: params.name,
      },
    ]
    this.mockRevision(patch)

    return null
  }
  private async addDevice(params: T.Tunnel.AddDeviceParams) {
    await pauseFor(1000)

    const patch: AddOperation<T.Tunnel.WgConfig>[] = [
      {
        op: PatchOp.ADD,
        path: `/wg/subnets/${replaceSlashes(params.subnet)}/clients/${params.ip}`,
        value: {
          name: params.name,
          key: '',
          psk: '',
          kind: params.kind,
          allowDnsInjection: params.kind === 'server',
          allowAutoPortForward: params.kind === 'server',
          wanIp: null,
        },
      },
    ]
    this.mockRevision(patch)

    return null
  }
  private async editDevice(params: T.Tunnel.AddDeviceParams) {
    await pauseFor(1000)

    const patch: ReplaceOperation<string>[] = [
      {
        op: PatchOp.REPLACE,
        path: `/wg/subnets/${replaceSlashes(params.subnet)}/clients/${params.ip}/name`,
        value: params.name,
      },
    ]
    this.mockRevision(patch)

    return null
  }
}

function replaceSlashes(val: string) {
  return val.replace(new RegExp('/', 'g'), '~1')
}

const MOCK_CONFIG = `# StartTunnel config for Phone

[Interface]
Address = 10.59.0.3/24
PrivateKey = 4K68mdpQWdEz/FpdVuRoZYgWpQgpW63J9GFzn+iOulQ=
DNS = 10.59.0.1

[Peer]
PublicKey = MQBiYHxAj7u8paj3L4w4uav3P/9YBPbaN4gkWn90SSs=
PresharedKey = 2JIBoK+Bxe7MJzX9zV+lFjqHxLTvehLp3piEROaNJjw=
AllowedIPs = 10.59.0.0/24
Endpoint = 203.0.113.42:51820
PersistentKeepalive = 25`
