import type { Api, ApiClient, Params, Result } from './api.types'
import { inject, Injectable } from '@angular/core'
import { WA_CRYPTO, WA_SESSION_STORAGE } from '@ng-web-apis/common'
import {
  FALLBACK_ICON,
  GetPackageRes,
  GetPackagesRes,
  registrySigningKeys,
} from '@start9labs/marketplace'
import {
  FullKeyboard,
  i18nPipe,
  pauseFor,
  registryUrl,
  sameUrl,
  withAbortSignal,
  RPCErrorDetails,
  SetLanguageParams,
} from '@start9labs/shared'
import {
  auth,
  createRpcClient,
  createRpcStreamClient,
  RpcHandlers,
  type RpcStreamOptions,
  T,
  tableTransport,
  utils,
} from '@start9labs/start-core'
import {
  AddOperation,
  applyOperation,
  Dump,
  Operation,
  PatchOp,
  pathFromArray,
  RemoveOperation,
  ReplaceOperation,
  Revision,
} from 'patch-db-client'
import {
  endWith,
  finalize,
  from,
  interval,
  map,
  Observable,
  shareReplay,
  startWith,
  Subject,
} from 'rxjs'
import { WebSocketSubject } from 'rxjs/webSocket'
import {
  BackupTargetEntry,
  DataModel,
  InstallingState,
  PackageDataEntry,
  StateInfo,
  UIData,
  UpdatingState,
} from 'src/app/services/patch-db/data-model'
import { ConfigService } from 'src/app/services/config.service'
import { PATCH_CACHE } from 'src/app/services/patch-db/patch-db-source'
import { toAuthorityUrl } from 'src/app/utils/acme'
import { getCurrentManifest, getManifest } from 'src/app/utils/get-package-data'
import { getDriveName } from 'src/app/utils/get-drive-name'
import { parsePemPublicKey, parsePublicKeys } from 'src/app/utils/public-keys'
import { BackupRun } from 'src/app/routes/portal/routes/system/routes/backups/backup.types'
import { Mock } from './api.fixures'
import { BTC_ICON, LND_ICON, PROXY_ICON } from './api-icons'
import {
  ActionRes,
  CheckDnsRes,
  DiagnosticErrorRes,
  FollowPackageLogsReq,
  FollowServerLogsReq,
  GetActionInputRes,
  GetPackageLogsReq,
  GetRegistryPackageReq,
  GetRegistryPackagesReq,
  PkgAddPrivateDomainReq,
  PkgAddPublicDomainReq,
  PkgBindingSetAddressEnabledReq,
  PkgBindingSetGuaWanReq,
  PkgRemovePrivateDomainReq,
  PkgRemovePublicDomainReq,
  ServerBindingSetAddressEnabledReq,
  ServerBindingSetGuaWanReq,
  ServerState,
  WebsocketConfig,
} from './api.types'
import { ApiService, FileUploadOptions } from './embassy-api.service'
import type { StartupApiService } from './startup-api.service'
import {
  mockOsUpdateCache,
  mockPatchData,
  mockRegistryCache,
} from './mock-patch'

import markdown from './md-sample.md'

const UI_KEY = '_startos/mock-ui'

type SourceCollection = 'sharedVolumes' | 'backupTargets'

const UNKNOWN_SOURCE: Result<'volumes.inspect'> = {
  availability: 'unknown',
  available: null,
  error: null,
  checkedAt: null,
}

const PROGRESS: T.FullProgress = {
  overall: {
    done: 0,
    total: 120,
    units: 'bytes',
  },
  phases: [
    {
      name: 'Downloading',
      progress: {
        done: 0,
        total: 40,
        units: 'bytes',
      },
    },
    {
      name: 'Validating',
      progress: null,
    },
    {
      name: 'Installing',
      progress: null,
    },
  ],
}

// Nested under finalization after download and validation complete.
const INIT_PROGRESS: T.FullProgress = {
  overall: { done: 0, total: 8, units: 'steps' },
  phases: [
    { name: 'Generating configuration', progress: false },
    { name: 'Migrating data', progress: { done: 0, total: 8, units: 'steps' } },
  ],
}

@Injectable()
export class MockApiService extends ApiService implements StartupApiService {
  readonly mockWsSource$ = new Subject<Revision>()
  private readonly i18n = inject(i18nPipe)
  private readonly storage = inject(WA_SESSION_STORAGE)
  private readonly cache = inject(PATCH_CACHE)
  private readonly gitHash = inject(ConfigService).gitHash
  private readonly revertTime = 1800
  private readonly crypto = inject(WA_CRYPTO)
  private drives?: Map<string, number>
  private history?: BackupRun[]
  private readonly conversionJobs: T.ServerStatus['backupConversions'] = {}

  private get attachedDrives() {
    return (this.drives ??= new Map(
      Mock.Disks.flatMap(disk => disk.partitions).map(partition => [
        Mock.driveLink(partition.logicalname),
        partition.available ?? partition.capacity,
      ]),
    ))
  }

  private get backupHistory() {
    return (this.history ??= structuredClone(Mock.BackupHistory))
  }

  private readonly pushSubscriptions = new Map<
    string,
    Params<'notification.push.subscribe'>
  >()
  sequence = 1
  private readonly handlers: RpcHandlers<Api> = {
    'server.set-nut': params => this.setNut(params),
    'server.nut-status': params => this.getNutStatus(params),
    'server.nut-list-ups': async () => {
      await pauseFor(1000)
      return [{ name: 'ups', description: 'Synology UPS' }]
    },
    'server.nut-scan': async () => {
      await pauseFor(2000)
      return [
        {
          driver: 'usbhid-ups',
          port: 'auto',
          vendorId: '051D',
          productId: '0002',
          serial: '4B1234P56789',
          bus: '001',
          busPort: '002',
          vendor: 'American Power Conversion',
          product: 'Back-UPS ES 600M1',
        },
      ]
    },
    'server.clear-nut': () =>
      this.setNut({ config: { enabled: false, settings: null } }),

    'git-info': async () => this.gitHash,
    state: async () => {
      await pauseFor(1000)

      this.stateIndex++

      return this.stateIndex === 1 ? 'running' : 'running'
    },
    'db.subscribe': async () => {
      await pauseFor(2000)

      const stored = this.storage?.getItem(UI_KEY)

      if (stored) {
        mockPatchData.ui = { ...mockPatchData.ui, ...JSON.parse(stored) }
      }

      mockPatchData.registryCache = Object.fromEntries(
        Object.keys(mockPatchData.serverInfo.registries).map(url => [
          registryUrl(url),
          mockRegistryCache(url, this.mockRegistryInfo(url)),
        ]),
      )
      mockPatchData.osUpdateCache = mockOsUpdateCache(
        mockPatchData.serverInfo.osRegistry,
      )
      this.protectTrust()
      return {
        dump: { id: this.sequence, value: mockPatchData },
        guid: 'db-guid',
      }
    },
    'db.put.ui': async ({ pointer, value }) => {
      await pauseFor(2000)

      const ui: Dump<UIData> = { id: 0, value: mockPatchData.ui }

      applyOperation(ui, { op: PatchOp.REPLACE, path: pointer, value })
      mockPatchData.ui = ui.value
      this.storage?.setItem(UI_KEY, JSON.stringify(ui.value))
      this.mockRevision([{ op: PatchOp.REPLACE, path: '/ui' + pointer, value }])

      return null
    },
    'auth.login': async params => {
      await pauseFor(2000)
      return null
    },
    'auth.logout': async params => {
      await pauseFor(2000)
      return null
    },
    'auth.session.list': async params => {
      await pauseFor(2000)
      return Mock.Sessions
    },
    'auth.session.kill': async params => {
      await pauseFor(2000)
      return null
    },
    'auth.reset-password': async params => {
      await pauseFor(2000)
      return null
    },
    'diagnostic.error': async () => {
      await pauseFor(1000)
      return {
        code: 15,
        message: 'Unknown server',
        data: { details: 'Some details about the error here' },
      }
    },
    'diagnostic.restart': async () => {
      await pauseFor(1000)
      return null
    },
    'diagnostic.disk.forget': async () => {
      await pauseFor(1000)
      return null
    },
    'diagnostic.disk.repair': async () => {
      await pauseFor(1000)
      return null
    },
    'diagnostic.update': () => this.diagnosticUpdate(),
    'diagnostic.logs': async params => {
      return this.handlers['server.logs']!(params)
    },
    'init.subscribe': () => this.initSubscription(),
    'init.logs.follow': () => this.logFollow(),
    'server.time': async params => {
      await pauseFor(2000)
      return {
        now: new Date().toUTCString(),
        uptime: 1234567,
      }
    },
    'server.logs': async params => {
      await pauseFor(2000)
      const entries = this.randomLogs(params.limit ?? undefined)

      return {
        entries,
        startCursor: 'start-cursor',
        endCursor: 'end-cursor',
      }
    },
    'server.kernel-logs': async params => {
      await pauseFor(2000)
      const entries = this.randomLogs(params.limit ?? undefined)

      return {
        entries,
        startCursor: 'start-cursor',
        endCursor: 'end-cursor',
      }
    },
    'server.logs.follow': () => this.logFollow(),
    'server.kernel-logs.follow': () => this.logFollow(),
    'server.metrics.follow': () => this.metricsFollow(),
    'server.device-info': async () => ({
      os: {
        version: mockPatchData.serverInfo.version,
        compat: mockPatchData.serverInfo.packageVersionCompat,
        platform: 'x86_64',
        language: mockPatchData.serverInfo.language,
      },
      hardware: null,
    }),
    'server.registries.add': async ({ url, name }) => {
      await pauseFor(2000)
      const normalized = registryUrl(url)
      const info = this.mockRegistryInfo(normalized)
      mockPatchData.registryCache[normalized] = mockRegistryCache(
        normalized,
        info,
      )
      const { added } = this.importRegistryKeys(normalized, info)
      for (const id of added)
        mockPatchData.serverInfo.trustedKeys[id]!.marketplace = true
      for (const saved of Object.keys(mockPatchData.serverInfo.registries)) {
        if (sameUrl(saved, normalized))
          delete mockPatchData.serverInfo.registries[saved]
      }
      mockPatchData.serverInfo.registries[normalized] = name ?? info.name
      await this.publishTrust()
      return added
    },
    'server.registries.refresh': async ({ url }) => {
      await pauseFor(2000)
      const normalized = registryUrl(url)
      if (
        !Object.keys(mockPatchData.serverInfo.registries).some(saved =>
          sameUrl(saved, normalized),
        )
      )
        throw new Error(
          this.i18n.transform('Connect this registry before refreshing it.'),
        )
      const info = this.mockRegistryInfo(normalized)
      mockPatchData.registryCache[normalized] = mockRegistryCache(
        normalized,
        info,
      )
      this.importRegistryKeys(normalized, info)
      await this.publishTrust()
      return null
    },
    'server.registries.remove': async ({ url }) => {
      await pauseFor(2000)
      if (sameUrl(url, mockPatchData.serverInfo.osRegistry))
        throw new Error(
          'This registry provides StartOS updates. Choose another registry for StartOS updates before removing it.',
        )
      const offered = this.offeredKeys(url)
      for (const saved of Object.keys(mockPatchData.registryCache)) {
        if (sameUrl(saved, url)) delete mockPatchData.registryCache[saved]
      }
      for (const saved of Object.keys(mockPatchData.serverInfo.registries)) {
        if (sameUrl(saved, url))
          delete mockPatchData.serverInfo.registries[saved]
      }
      for (const saved of Object.keys(mockPatchData.serverInfo.registryKeys)) {
        if (sameUrl(saved, url))
          delete mockPatchData.serverInfo.registryKeys[saved]
      }
      await this.publishTrust()
      return offered.filter(id => {
        const key = mockPatchData.serverInfo.trustedKeys[id]
        return key?.removable && key.origin !== 'manual'
      })
    },
    'server.registries.set-os': async ({ url }) => {
      await pauseFor(2000)
      if (
        !Object.keys(mockPatchData.serverInfo.registries).some(saved =>
          sameUrl(saved, url),
        )
      )
        throw new Error('Add this registry before using it for StartOS updates')
      const enabled = this.offeredKeys(url).filter(
        id => mockPatchData.serverInfo.trustedKeys[id]?.os === false,
      )
      for (const id of enabled)
        mockPatchData.serverInfo.trustedKeys[id]!.os = true
      mockPatchData.serverInfo.osRegistry = registryUrl(url)
      mockPatchData.osUpdateCache = mockOsUpdateCache(url)
      await this.publishTrust()
      return enabled
    },
    'server.trusted-keys.add': async ({ name, publicKeys, trustFor }) => {
      await pauseFor(2000)
      const keys = structuredClone(mockPatchData.serverInfo.trustedKeys)
      const added: string[] = []
      const existing: string[] = []
      for (const key of parsePublicKeys(publicKeys.join('\n'))) {
        const id = auth.normalizeDer64(parsePemPublicKey(key))
        if (keys[id]) {
          existing.push(id)
        } else {
          added.push(id)
          keys[id] = {
            name,
            origin: 'manual',
            originRegistry: null,
            gpgExports: [],
            contact: [],
            addedAt: new Date().toISOString(),
            publicKey: id,
            removable: true,
            marketplace: true,
            os: false,
          }
        }
        if (trustFor) keys[id]![trustFor] = true
      }
      mockPatchData.serverInfo.trustedKeys = keys
      await this.publishTrust()
      return { added, existing }
    },
    'server.trusted-keys.update': async ({ id, name, marketplace, os }) => {
      await pauseFor(2000)
      const key = mockPatchData.serverInfo.trustedKeys[id]
      if (!key) throw new Error('Key not found')
      if (!os) this.keepOsKeys([id])
      const value = { ...key, name, marketplace, os }
      mockPatchData.serverInfo.trustedKeys[id] = value
      await this.mockRevision([
        {
          op: PatchOp.REPLACE,
          path: pathFromArray(['serverInfo', 'trustedKeys', id]),
          value,
        },
      ])
      return null
    },
    'server.trusted-keys.remove': async ({ id }) => this.removeKeys([id]),
    'server.trusted-keys.remove-many': async ({ ids }) => this.removeKeys(ids),
    'server.update': async params => {
      await pauseFor(2000)
      const initialProgress = {
        size: null,
        downloaded: 0,
      }

      setTimeout(() => {
        this.updateOSProgress()
      }, 500)

      const patch = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/statusInfo/updateProgress',
          value: initialProgress,
        },
      ]
      this.mockRevision(patch)

      return {
        target: params.targetVersion?.replace(/^=/, '') ?? null,
        progress: null,
      }
    },
    'server.restart': async params => {
      await pauseFor(2000)

      const patch = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/statusInfo/restarting',
          value: true,
        },
      ]
      this.mockRevision(patch)

      setTimeout(() => {
        const patch2 = [
          {
            op: PatchOp.REPLACE,
            path: '/serverInfo/statusInfo/restarting',
            value: false,
          },
        ]
        this.mockRevision(patch2)
      }, 2000)

      return null
    },
    'server.shutdown': async params => {
      await pauseFor(2000)

      const patch = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/statusInfo/shuttingDown',
          value: true,
        },
      ]
      this.mockRevision(patch)

      setTimeout(() => {
        const patch2 = [
          {
            op: PatchOp.REPLACE,
            path: '/serverInfo/statusInfo/shuttingDown',
            value: false,
          },
        ]
        this.mockRevision(patch2)
      }, 2000)

      return null
    },
    'disk.repair': async params => {
      await pauseFor(2000)
      return null
    },
    'disk.format': async ({ disk, label }) => {
      await pauseFor(3000)
      const partition = `${disk}1`
      const drive = Mock.Disks.find(d => d.logicalname === disk)!
      this.attachedDrives.set(Mock.driveLink(partition), drive.capacity)
      drive.partitions = [
        {
          logicalname: partition,
          stablePath: `${drive.stablePath}-part1`,
          label: label ?? 'StartOS Backups',
          capacity: drive.capacity,
          used: 0,
          available: drive.capacity,
          startOs: {},
          legacyBackup: false,
          guid: null,
          filesystem: 'btrfs',
        },
      ]
      await this.publishDisks()
      return partition
    },
    'server.set-hostname': async params => {
      await pauseFor(1000)

      this.mockRevision([
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/hostname',
          value: params.hostname,
        },
      ])
      this.mockRevision([
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/statusInfo/restart',
          value: 'mdns',
        },
      ])

      return null
    },
    'server.set-keyboard': async params => {
      await pauseFor(1000)

      const patch = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/keyboard',
          value: params,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'server.set-language': async params => {
      await pauseFor(1000)

      this.mockRevision([
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/language',
          value: params.language,
        },
      ])
      this.mockRevision([
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/statusInfo/restart',
          value: 'language',
        },
      ])

      return null
    },
    'net.dns.status': async () => ({ bootstrap: false, gateway: null }),
    'net.dns.set-static': async params => {
      await pauseFor(2000)

      const dns = this.mockGatewayDns(params.gateway, params.servers ?? null)
      const patch: ReplaceOperation<T.DnsSettings>[] = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/network/gateways/' + params.gateway + '/dns',
          value: dns,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'net.dns.query': async params => {
      await pauseFor(2000)

      return { ipv4: null, ipv6: null }
    },
    'net.gateway.check-port': async params => {
      await pauseFor(2000)

      return {
        ip: '0.0.0.0',
        port: params.port,
        openExternally: true,
        openInternally: false,
        hairpinning: true,
      }
    },
    'net.gateway.check-port-v6': async params => {
      await pauseFor(2000)

      return {
        ip: '::',
        openExternally: true,
        openInternally: false,
      }
    },
    'net.acme.check-challenge': async params => {
      await pauseFor(2000)

      return {
        port: {
          ip: '0.0.0.0',
          port: 443,
          openExternally: false,
          openInternally: false,
          hairpinning: false,
        },
        portV6: null,
      }
    },
    'net.gateway.check-dns': async params => {
      await pauseFor(2000)

      return false
    },
    'registry.os.version.get': async params => {
      await pauseFor(2000)
      return mockOsUpdateCache(params.registry).versions
    },
    'registry.info': async params => {
      await pauseFor(2000)
      return Mock.RegistryInfo
    },
    'notification.push.key': async () =>
      'BMPJ4K-bgv51B65CU1VyQTEkdx1KpAEwNW1yBT1s1L897jXuwArMYD6evcHvlejf27P-ouK7X_LwifLqyBkL6Hs',
    'notification.push.list': async () => [...this.pushSubscriptions.values()],
    'notification.push.subscribe': async params => {
      this.pushSubscriptions.set(params.endpoint, params)
      return null
    },
    'notification.push.unsubscribe': async ({ endpoint }) => {
      this.pushSubscriptions.delete(endpoint)
      return null
    },
    'notification.push.test': async () => null,
    'notification.list': async params => {
      await pauseFor(2000)

      return Mock.Notifications
    },
    'notification.remove': async params => {
      await pauseFor(2000)
      return null
    },
    'notification.mark-seen': async params => {
      await pauseFor(2000)
      return null
    },
    'notification.mark-seen-before': async params => {
      await pauseFor(2000)
      return null
    },
    'notification.mark-unseen': async params => {
      await pauseFor(2000)
      return null
    },
    'net.tunnel.add': async params => {
      await pauseFor(2000)

      const id = `wg${this.proxyId++}`

      const patch: AddOperation<any>[] = [
        {
          op: PatchOp.ADD,
          path: `/serverInfo/network/gateways/${id}`,
          value: {
            name: params.name,
            secure: false,
            wireguardEndpointDomains: [],
            dnsUpdate: { supported: null, at: null },
            portMap: {
              pcp: { supported: null, at: null },
              natPmp: { supported: null, at: null },
              upnp: { supported: null, at: null },
              pcpHostname: { supported: null, at: null },
            },
            dns: {
              staticServers: null,
              servers: ['1.1.1.1:53', '[2606:4700:4700::1111]:53'],
              source: 'public-fallback',
            },
            ipInfo: {
              name: id,
              scopeId: 3,
              deviceType: 'wireguard',
              subnets: ['192.168.1.10/24'],
              wanIp: '203.0.113.45',
              ntpServers: [],
              lanIp: ['192.168.1.10'],
              dnsServers: [],
              defaultRouteV4: null,
              defaultRouteV6: null,
            },
            type:
              params.type ??
              (params.config.toLowerCase().includes('starttunnel')
                ? 'inbound-outbound'
                : 'outbound-only'),
          },
        },
      ]

      if (params.setAsDefaultOutbound) {
        ;(patch as any[]).push({
          op: PatchOp.REPLACE,
          path: '/serverInfo/network/defaultOutbound',
          value: id,
        })
      }

      this.mockRevision(patch)

      return id
    },
    'net.gateway.set-name': async params => {
      await pauseFor(2000)
      this.mockRevision([
        {
          op: PatchOp.REPLACE,
          path: `/serverInfo/network/gateways/${params.id}/name`,
          value: params.name,
        },
      ])
      return null
    },
    'net.gateway.edit': async params => {
      await pauseFor(2000)
      const base = `/serverInfo/network/gateways/${params.id}`
      if (!this.mockData(base)) throw new Error('Unknown gateway')
      const patch: Operation<any>[] = []
      if (params.dns)
        patch.push({
          op: PatchOp.REPLACE,
          path: `${base}/dns`,
          value: this.mockGatewayDns(params.id, params.dns.servers ?? null),
        })
      if (params.name != null)
        patch.push({
          op: PatchOp.REPLACE,
          path: `${base}/name`,
          value: params.name,
        })
      this.mockRevision(patch)
      return null
    },
    'net.tunnel.update': async params => {
      await pauseFor(2000)
      return null
    },
    'net.tunnel.remove': async params => {
      await pauseFor(2000)
      const patch: RemoveOperation[] = [
        {
          op: PatchOp.REMOVE,
          path: `/serverInfo/network/gateways/${params.id}`,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'net.gateway.set-default-outbound': async params => {
      await pauseFor(2000)
      const patch: Operation<any>[] = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/network/defaultOutbound',
          value: params.gateway,
        },
      ]
      if (params.dns)
        patch.push({
          op: PatchOp.REPLACE,
          path: '/serverInfo/network/outboundDns',
          value: this.mockDnsServers(params.dns.servers ?? null),
        })
      this.mockRevision(patch)
      return null
    },
    'package.set-outbound-gateway': async params => {
      await pauseFor(2000)
      const patch: Operation<any>[] = [
        {
          op: PatchOp.REPLACE,
          path: `/packageData/${params.package}/outboundGateway`,
          value: params.gateway,
        },
      ]
      if (params.dns)
        patch.push({
          op: PatchOp.REPLACE,
          path: `/packageData/${params.package}/outboundDns`,
          value: this.mockDnsServers(params.dns.servers ?? null),
        })
      this.mockRevision(patch)
      return null
    },
    'wifi.set-enabled': async params => {
      await pauseFor(2000)
      const patch = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/network/wifi/enabled',
          value: params.enabled,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'wifi.country.set': async params => {
      await pauseFor(2000)
      return null
    },
    'wifi.get': async params => {
      await pauseFor(2000)
      return Mock.Wifi
    },
    'wifi.add': async params => {
      await pauseFor(2000)
      return null
    },
    'wifi.connect': async params => {
      await pauseFor(2000)
      return null
    },
    'wifi.remove': async params => {
      await pauseFor(2000)
      return null
    },
    'server.set-smtp': async params => {
      await pauseFor(2000)
      const patch = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/smtp',
          value: params,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'server.clear-smtp': async params => {
      await pauseFor(2000)
      const patch = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/smtp',
          value: null,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'server.test-smtp': async params => {
      await pauseFor(2000)
      return null
    },
    'ssh.list': async params => {
      await pauseFor(2000)
      return Mock.SshKeys
    },
    'ssh.add': async params => {
      await pauseFor(2000)
      return Mock.SshKey
    },
    'ssh.remove': async params => {
      await pauseFor(2000)
      return null
    },
    'volumes.create': async ({ name, source }) => {
      await pauseFor(1000)
      const id = this.guid()
      const volume: T.SharedVolume = {
        name,
        source: this.mockSource(source),
        grants: {},
        ...UNKNOWN_SOURCE,
      }
      await this.mockRevision<unknown>([
        {
          op: PatchOp.ADD,
          path: `/sharedVolumes/${id}`,
          value: volume,
        },
      ])
      this.queueSourceCheck('sharedVolumes', id)
      return id
    },
    'volumes.update-cifs': async ({ id, hostname, path, username }) => {
      await pauseFor(1000)
      const volume: T.SharedVolume | undefined = this.mockData(
        `/sharedVolumes/${id}`,
      )
      if (!volume || volume.source.kind !== 'cifs')
        throw new Error('CIFS volume not found')
      await this.mockRevision<unknown>([
        {
          op: PatchOp.REPLACE,
          path: `/sharedVolumes/${id}`,
          value: {
            ...volume,
            source: this.mockSource({ kind: 'cifs', hostname, path, username }),
            ...UNKNOWN_SOURCE,
          },
        },
      ])
      this.queueSourceCheck('sharedVolumes', id)
      return null
    },
    'volumes.rename': async ({ id, name }) => {
      const volume: T.SharedVolume | undefined = this.mockData(
        `/sharedVolumes/${id}`,
      )
      if (!volume) throw new Error('Volume not found')
      await this.mockRevision([
        { op: PatchOp.REPLACE, path: `/sharedVolumes/${id}/name`, value: name },
      ])
      return null
    },
    'volumes.set-grant': async ({ id, packageId, grant }) => {
      const volume: T.SharedVolume | undefined = this.mockData(
        `/sharedVolumes/${id}`,
      )
      if (!volume) throw new Error('Volume not found')
      if (grant) {
        const pkg = this.mockData(`/packageData/${packageId}`) as
          | PackageDataEntry
          | undefined
        if (!pkg || !getCurrentManifest(pkg).sharedVolumes)
          throw new Error('This service cannot use shared volumes')
      }
      const grants = { ...volume.grants }
      if (grant) grants[packageId] = grant
      else delete grants[packageId]
      await this.mockRevision([
        {
          op: PatchOp.REPLACE,
          path: `/sharedVolumes/${id}/grants`,
          value: grants,
        },
      ])
      return null
    },
    'volumes.remove': async ({ id }) => {
      const volume: T.SharedVolume | undefined = this.mockData(
        `/sharedVolumes/${id}`,
      )
      if (!volume) return null
      if (volume.source.kind === 'local') {
        await this.mockRevision([
          {
            op: PatchOp.REPLACE,
            path: `/sharedVolumes/${id}/grants`,
            value: {},
          },
        ])
      }
      await this.mockRevision([
        { op: PatchOp.REMOVE, path: `/sharedVolumes/${id}` },
      ])
      return null
    },
    'volumes.inspect': async ({ id }) => {
      await pauseFor(1000)
      return this.checkSource('sharedVolumes', id)
    },
    'backup.target.list': async params => {
      await pauseFor(2000)
      return structuredClone(
        this.mockData('/backupTargets'),
      ) as DataModel['backupTargets']
    },
    'backup.history.list': async ({ offset, limit }) => {
      await pauseFor(500)
      return {
        runs: structuredClone(this.backupHistory.slice(offset, offset + limit)),
        total: this.backupHistory.length,
      }
    },
    'backup.target.convert': async params => {
      const backup: T.StartOsRecoveryInfo | undefined = structuredClone(
        this.backupTarget(params.targetId)?.startOs[params.serverId],
      )
      if (!backup) throw new Error('Backup not found')
      const jobs = (this.conversionJobs[params.targetId] ??= {})
      if (jobs[params.serverId]?.state === 'running')
        throw new Error('Conversion already running')
      jobs[params.serverId] = {
        state: 'running',
        progress: { overall: false, phases: [] },
        error: null,
      }
      backup.formatStatus = 'interrupted'
      await this.publishBackup(params, backup)
      void this.simulateConversion(params, backup)
      return null
    },
    'backup.target.create': async ({ name, source }) => {
      await pauseFor(1000)
      const id = this.guid()
      await this.mockRevision<unknown>([
        {
          op: PatchOp.ADD,
          path: `/backupTargets/${id}`,
          value: {
            name,
            source: this.mockSource(source),
            ...UNKNOWN_SOURCE,
            startOs: {},
            legacyBackup: false,
          },
        },
      ])
      this.queueSourceCheck('backupTargets', id)
      return id
    },
    'backup.target.find': async () => {
      await this.publishDisks()
      const added: string[] = []
      for (const partition of Mock.Disks.flatMap(disk => disk.partitions)) {
        if (!Object.keys(partition.startOs).length) continue
        const storedSource = Mock.diskSource(partition.logicalname)
        const logicalname = storedSource.logicalname
        const targets: Record<string, T.BackupTarget> =
          this.mockData('/backupTargets')
        if (
          Object.values(targets).some(
            ({ source }) =>
              source.kind === 'disk' &&
              source.logicalname === logicalname &&
              !source.path,
          )
        )
          continue
        added.push(
          await this.rpc.backup.target.create({
            name: partition.label?.trim()
              ? partition.label
              : getDriveName(storedSource.drive) || partition.logicalname,
            source: {
              kind: 'disk',
              logicalname: partition.logicalname,
              path: '',
            },
          }),
        )
      }
      return added
    },
    'backup.target.rename': async ({ id, name }) => {
      await this.mockRevision<unknown>([
        { op: PatchOp.REPLACE, path: `/backupTargets/${id}/name`, value: name },
      ])
      return null
    },
    'backup.target.update-cifs': async ({ id, hostname, path, username }) => {
      await pauseFor(1000)
      const target = this.cifsTarget(id)
      await this.mockRevision<unknown>([
        {
          op: PatchOp.REPLACE,
          path: `/backupTargets/${id}`,
          value: {
            ...target,
            source: this.mockSource({ kind: 'cifs', hostname, path, username }),
            ...UNKNOWN_SOURCE,
            startOs: {},
            legacyBackup: false,
          },
        },
      ])
      this.queueSourceCheck('backupTargets', id)
      return null
    },
    'backup.target.remove': async ({ id }) => {
      await pauseFor(1000)
      if (this.backupTarget(id))
        await this.mockRevision([
          { op: PatchOp.REMOVE, path: `/backupTargets/${id}` },
        ])
      return null
    },
    'backup.target.inspect': async ({ id }) => {
      await pauseFor(1000)
      return this.checkSource('backupTargets', id)
    },
    'backup.target.cifs.add': async params => {
      const id = await this.rpc.backup.target.create({
        name: `${params.hostname}:${params.path}`,
        source: { kind: 'cifs', ...params },
      })
      return { [id]: this.backupTarget(id)! }
    },
    'backup.target.cifs.update': async params => {
      await this.rpc.backup.target.updateCifs(params)
      return { [params.id]: this.backupTarget(params.id)! }
    },
    'backup.target.cifs.remove': async params => {
      this.cifsTarget(params.id)
      return this.rpc.backup.target.remove(params)
    },
    'backup.target.delete-legacy': async params => {
      await pauseFor(2000)
      await this.mockRevision<unknown>([
        {
          op: PatchOp.REPLACE,
          path: `/backupTargets/${params.targetId}/legacyBackup`,
          value: false,
        },
      ])
      return null
    },
    'backup.target.info': async params => {
      await pauseFor(2000)
      return Mock.BackupInfo
    },
    'backup.create': async params => {
      const startedAt = new Date().toISOString()
      await pauseFor(2000)
      const target = this.backupTarget(params.targetId)
      if (!target) throw new Error('Backup target not found')
      const targetName = target.name
      const source = structuredClone(target.source)
      const serverPath = '/serverInfo/statusInfo/backupProgress'
      const ids =
        params.packageIds ?? Object.keys(this.mockData('/packageData'))
      const volumes =
        params.sharedVolumeIds ?? Object.keys(this.mockData('/sharedVolumes'))
      const report: T.BackupReport = {
        server: { attempted: true, error: null },
        packages: {},
        sharedVolumes: {},
      }
      const phaseCount = ids.length + volumes.length + 1

      setTimeout(async () => {
        for (let i = 0; i < ids.length; i++) {
          const id = ids[i]!
          const appPath = `/packageData/${id}/statusInfo/desired/main`
          const previous = this.mockData(appPath) as T.DesiredStatus['main']
          const appPatch: ReplaceOperation<T.DesiredStatus['main']>[] = [
            {
              op: PatchOp.REPLACE,
              path: appPath,
              value: 'backing-up',
            },
          ]
          this.mockRevision(appPatch)

          for (const pct of [25, 50, 75]) {
            await pauseFor(2000)
            this.mockRevision([
              {
                op: PatchOp.REPLACE,
                path: `${serverPath}/phases/${i}/progress`,
                value: { done: pct, total: 100, units: null },
              },
            ])
          }

          await pauseFor(2000)

          this.mockRevision([
            {
              ...appPatch[0]!,
              value: previous,
            },
          ])

          const completePhase: ReplaceOperation<T.Progress> = {
            op: PatchOp.REPLACE,
            path: `${serverPath}/phases/${i}/progress`,
            value: true,
          }
          const advanceOverall: ReplaceOperation<T.Progress> = {
            op: PatchOp.REPLACE,
            path: `${serverPath}/overall`,
            value: { done: i + 1, total: phaseCount, units: null },
          }
          this.mockRevision([completePhase, advanceOverall])
          const manifest = getManifest(this.mockData(`/packageData/${id}`))
          Mock.BackupInfo.packageBackups[id] = {
            title: manifest.title,
            version: manifest.version,
            osVersion: this.mockData('/serverInfo/version'),
            timestamp: new Date().toISOString(),
          }
          report.packages[id] = { error: null, duration_ms: 8000 }
        }
        for (const [offset, id] of volumes.entries()) {
          const volume = this.mockData(`/sharedVolumes/${id}`) as
            | T.SharedVolume
            | undefined
          const inspection = volume
            ? await this.rpc.volumes.inspect({ id })
            : null
          const error =
            inspection?.error ?? (volume ? null : 'Volume not found')
          report.sharedVolumes[id] = {
            name: volume?.name ?? id,
            error,
            duration_ms: 1000,
          }
          if (!error && volume)
            Mock.BackupInfo.sharedVolumeBackups[id] = {
              name: volume.name,
              timestamp: new Date().toISOString(),
              generation: this.guid(),
            }
          await pauseFor(1000)
          await this.mockRevision<T.Progress>([
            {
              op: PatchOp.REPLACE,
              path: `${serverPath}/phases/${ids.length + offset}/progress`,
              value: true,
            },
            {
              op: PatchOp.REPLACE,
              path: `${serverPath}/overall`,
              value: {
                done: ids.length + offset + 1,
                total: phaseCount,
                units: null,
              },
            },
          ])
        }

        const osIdx = ids.length + volumes.length
        await pauseFor(1500)
        const completeOsPhase: ReplaceOperation<T.Progress> = {
          op: PatchOp.REPLACE,
          path: `${serverPath}/phases/${osIdx}/progress`,
          value: true,
        }
        const completeOverall: ReplaceOperation<T.Progress> = {
          op: PatchOp.REPLACE,
          path: `${serverPath}/overall`,
          value: true,
        }
        this.mockRevision([completeOsPhase, completeOverall])

        await pauseFor(800)

        this.backupHistory.unshift({
          id: this.guid(),
          targetId: params.targetId,
          targetName,
          startedAt,
          completedAt: new Date().toISOString(),
          report,
        })

        const lastPatch: ReplaceOperation<T.ServerStatus['backupProgress']>[] =
          [
            {
              op: PatchOp.REPLACE,
              path: serverPath,
              value: null,
            },
          ]
        this.mockRevision(lastPatch)
        if (utils.deepEqual(this.backupTarget(params.targetId)?.source, source))
          await this.mockRevision<unknown>([
            {
              op: PatchOp.ADD,
              path: `/backupTargets/${params.targetId}/startOs/${Mock.ServerId}`,
              value: {
                hostname: this.mockData('/serverInfo/hostname'),
                timestamp: new Date().toISOString(),
                version: this.mockData('/serverInfo/version'),
                formatStatus: 'current',
              },
            },
          ])
      }, 500)

      const originalPatch: ReplaceOperation<
        T.ServerStatus['backupProgress']
      >[] = [
        {
          op: PatchOp.REPLACE,
          path: serverPath,
          value: {
            overall: { done: 0, total: phaseCount, units: null },
            phases: [
              ...ids.map(id => ({ name: id, progress: null })),
              ...volumes.map(id => ({
                name: this.mockData(`/sharedVolumes/${id}`)?.name ?? id,
                progress: null,
              })),
              { name: 'OS Data', progress: null },
            ],
          },
        },
      ]

      this.mockRevision(originalPatch)

      return null
    },
    'package.logs': async params => {
      await pauseFor(2000)
      let entries
      if (Math.random() < 0.2) {
        entries = Mock.ServerLogs
      } else {
        const arrLength = params.limit
          ? Math.ceil(params.limit / Mock.ServerLogs.length)
          : 10
        entries = new Array(arrLength)
          .fill(Mock.ServerLogs)
          .reduce((acc, val) => acc.concat(val), [])
      }
      return {
        entries,
        startCursor: 'startCursor',
        endCursor: 'end-cursor',
      }
    },
    'package.logs.follow': () => this.logFollow(),
    'package.install': async params => {
      await pauseFor(2000)

      const existing = mockPatchData.packageData[params.id]

      let value: PackageDataEntry<InstallingState | UpdatingState>
      let finalManifest: T.Manifest
      if (existing && existing.stateInfo.state === 'installed') {
        // Updating: preserve the installed manifest so LocalPackagesService
        // keeps the row visible and the item component renders its progress
        // circle. Previously we always used the 'installing' state here, which
        // dropped the row out of localPkgs mid-update and left the Updates tab
        // briefly showing "Finding suitable version..." before the row vanished.
        const manifest = existing.stateInfo.manifest
        finalManifest = { ...manifest, version: params.version }
        value = {
          ...existing,
          stateInfo: {
            state: 'updating',
            manifest,
            s9pk: existing.s9pk,
            installingInfo: {
              newManifest: finalManifest,
              progress: PROGRESS,
            },
          },
        }
      } else {
        // Fresh install: no prior manifest to preserve.
        finalManifest = Mock.LocalPkgs[params.id]?.stateInfo.manifest!
        value = {
          ...Mock.LocalPkgs[params.id]!,
          stateInfo: {
            state: 'installing',
            installingInfo: {
              newManifest: finalManifest,
              progress: PROGRESS,
            },
          },
        }
      }

      setTimeout(async () => {
        this.installProgress(params.id, finalManifest)
      }, 1000)

      const patch: AddOperation<
        PackageDataEntry<InstallingState | UpdatingState>
      >[] = [
        {
          op: PatchOp.ADD,
          path: `/packageData/${params.id}`,
          value,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'package.cancel-install': async params => {
      await pauseFor(500)

      const patch: RemoveOperation[] = [
        {
          op: PatchOp.REMOVE,
          path: `/packageData/${params.id}`,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'package.action.get-input': async params => {
      await pauseFor(2000)

      if (
        params.packageId === 'tor' &&
        params.actionId === 'create-onion-service'
      ) {
        return {
          eventId: 'ANZXNWIFRTTBZ6T52KQPZILIQQODDHXQ',
          value: null,
          spec: await Mock.getCreateOnionServiceSpec(),
        }
      }

      if (params.actionId === 'rpc') {
        return {
          eventId: 'ANZXNWIFRTTBZ6T52KQPZILIQQODDHXQ',
          value: null,
          spec: await Mock.getRpcSpec(),
        }
      }

      if (params.actionId === 'file-upload') {
        return {
          eventId: 'ANZXNWIFRTTBZ6T52KQPZILIQQODDHXQ',
          value: null,
          spec: await Mock.getFileInputSpec(),
        }
      }

      return {
        eventId: 'ANZXNWIFRTTBZ6T52KQPZILIQQODDHXQ',
        value: Mock.MockConfig,
        spec: await Mock.getActionInputSpec(),
      }
    },
    'package.action.run': async params => {
      await pauseFor(2000)

      const patch: ReplaceOperation<{ [key: string]: T.TaskEntry }>[] = [
        {
          op: PatchOp.REPLACE,
          path: `/packageData/${params.packageId}/tasks`,
          value: {},
        },
      ]
      this.mockRevision(patch)

      if (params.actionId === 'file-result')
        return {
          version: '1',
          title: 'Diagnostics',
          message: 'Download the generated report.',
          result: { type: 'file', value: 'mock-download', delete: true },
        }
      if (params.actionId === 'big-qr') return Mock.ActionResBigQr
      if (params.actionId === 'unencodable-qr')
        return Mock.ActionResUnencodableQr
      if (params.actionId === 'multiline') return Mock.ActionResMultiline
      if (params.actionId === 'multiline-secret')
        return Mock.ActionResMultilineSecret
      if (params.actionId === 'multiline-group')
        return Mock.ActionResMultilineGroup
      return Mock.ActionResMessage
    },
    'package.action.clear-task': async params => {
      await pauseFor(2000)

      const patch: RemoveOperation[] = [
        {
          op: PatchOp.REMOVE,
          path: `/packageData/${params.packageId}/tasks/${params.replayId}`,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'backup.volume.restore': async ({ ids }) => {
      const result: Result<'backup.volume.restore'> = {
        restored: {},
        errors: {},
      }
      for (const id of ids ??
        Object.keys(Mock.BackupInfo.sharedVolumeBackups)) {
        const info = Mock.BackupInfo.sharedVolumeBackups[id]
        if (!info) {
          result.errors[id] = 'Volume not found in backup'
          continue
        }
        const restored = await this.rpc.volumes.create({
          name: info.name,
          source: { kind: 'local' },
        })
        await this.mockRevision([
          {
            op: PatchOp.REPLACE,
            path: `/sharedVolumes/${restored}/availability`,
            value: 'available',
          },
        ])
        result.restored[id] = restored
      }
      return result
    },
    'package.backup.restore': async params => {
      await pauseFor(2000)
      const patch: AddOperation<PackageDataEntry>[] = params.ids.map(id => {
        setTimeout(async () => {
          this.installProgress(id)
        }, 2000)

        return {
          op: PatchOp.ADD,
          path: `/packageData/${id}`,
          value: {
            ...Mock.LocalPkgs[id]!,
            stateInfo: {
              state: 'restoring',
              installingInfo: {
                newManifest: Mock.LocalPkgs[id]?.stateInfo.manifest!,
                progress: PROGRESS,
              },
            },
          },
        }
      })

      this.mockRevision(patch)

      return null
    },
    'package.start': async params => {
      const path = `/packageData/${params.id}/statusInfo`

      await pauseFor(2000)

      setTimeout(async () => {
        const patch2: ReplaceOperation<T.StatusInfo>[] = [
          {
            op: PatchOp.REPLACE,
            path,
            value: {
              error: null,
              desired: { main: 'running' },
              started: new Date().toISOString(),
              health: {
                'ephemeral-health-check': {
                  name: 'Ephemeral Health Check',
                  result: 'success',
                  message: null,
                },
                'unnecessary-health-check': {
                  name: 'Unnecessary Health Check',
                  result: 'disabled',
                  message: 'Custom disabled message',
                },
                'chain-state': {
                  name: 'Chain State',
                  result: 'loading',
                  waiting: false,
                  message:
                    'Bitcoin is syncing from genesis. Downloading block headers and verifying chain integrity, please wait',
                },
                'p2p-interface': {
                  name: 'P2P Interface',
                  result: 'success',
                  message: null,
                },
                'rpc-interface': {
                  name: 'RPC Interface',
                  result: 'failure',
                  message: 'Custom failure message',
                },
              },
            },
          },
        ]
        this.mockRevision(patch2)
      }, 2000)

      const originalPatch: ReplaceOperation<T.StatusInfo>[] = [
        {
          op: PatchOp.REPLACE,
          path,
          value: {
            desired: { main: 'running' },
            started: null,
            error: null,
            health: {},
          },
        },
      ]

      this.mockRevision(originalPatch)

      return null
    },
    'package.restart': async params => {
      await pauseFor(2000)
      const path = `/packageData/${params.id}/statusInfo`

      setTimeout(async () => {
        const patch2: ReplaceOperation<T.StatusInfo>[] = [
          {
            op: PatchOp.REPLACE,
            path,
            value: {
              desired: { main: 'running' },
              error: null,
              started: new Date().toISOString(),
              health: {
                'ephemeral-health-check': {
                  name: 'Ephemeral Health Check',
                  result: 'starting',
                  waiting: false,
                  message: null,
                },
                'unnecessary-health-check': {
                  name: 'Unnecessary Health Check',
                  result: 'disabled',
                  message: 'Custom disabled message',
                },
                'chain-state': {
                  name: 'Chain State',
                  result: 'loading',
                  waiting: false,
                  message: 'Bitcoin is syncing from genesis',
                },
                'p2p-interface': {
                  name: 'P2P Interface',
                  result: 'success',
                  message: null,
                },
                'rpc-interface': {
                  name: 'RPC Interface',
                  result: 'failure',
                  message: 'Custom failure message',
                },
              },
            },
          },
        ]
        this.mockRevision(patch2)
      }, this.revertTime)

      const patch: ReplaceOperation<T.StatusInfo>[] = [
        {
          op: PatchOp.REPLACE,
          path,
          value: {
            desired: { main: 'restarting', restartAgain: false },
            started: null,
            error: null,
            health: {},
          },
        },
      ]

      this.mockRevision(patch)

      return null
    },
    'package.stop': async params => {
      await pauseFor(2000)
      const path = `/packageData/${params.id}/statusInfo`

      setTimeout(() => {
        const patch2: ReplaceOperation<T.StatusInfo>[] = [
          {
            op: PatchOp.REPLACE,
            path: path,
            value: {
              desired: { main: 'stopped' },
              error: null,
              health: {},
              started: null,
            },
          },
        ]
        this.mockRevision(patch2)
      }, this.revertTime)

      const patch: ReplaceOperation<T.StatusInfo>[] = [
        {
          op: PatchOp.REPLACE,
          path: path,
          value: {
            desired: { main: 'stopped' },
            error: null,
            health: {},
            started: new Date().toISOString(),
          },
        },
      ]

      this.mockRevision(patch)

      return null
    },
    'package.rebuild': async params => {
      return this.handlers['package.restart']!(params)
    },
    'package.uninstall': async params => {
      await pauseFor(2000)

      setTimeout(async () => {
        const patch2: RemoveOperation[] = [
          {
            op: PatchOp.REMOVE,
            path: `/packageData/${params.id}`,
          },
        ]
        this.mockRevision(patch2)
      }, this.revertTime)

      const patch: ReplaceOperation<T.PackageState['state']>[] = [
        {
          op: PatchOp.REPLACE,
          path: `/packageData/${params.id}/stateInfo/state`,
          value: 'removing',
        },
      ]

      this.mockRevision(patch)

      return null
    },
    'package.sideload': async () => {
      await pauseFor(2000)
      return {
        upload: 'sideload-upload-guid', // no significance, randomly generated
        progress: 'sideload-progress-guid', // no significance, randomly generated
      }
    },
    'net.acme.init': async params => {
      await pauseFor(2000)

      const patch = [
        {
          op: PatchOp.ADD,
          path: `/serverInfo/acme`,
          value: {
            [toAuthorityUrl(params.provider)]: { contact: params.contact },
          },
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'net.acme.remove': async params => {
      await pauseFor(2000)

      const regex = new RegExp('/', 'g')

      const patch: RemoveOperation[] = [
        {
          op: PatchOp.REMOVE,
          path: `/serverInfo/acme/${params.provider.replace(regex, '~1')}`,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'server.host.binding.set-address-enabled': async params => {
      await pauseFor(2000)

      const basePath = `/serverInfo/network/host/bindings/${params.internalPort}/addresses`
      this.mockSetAddressEnabled(basePath, params.address, params.enabled)

      return null
    },
    'server.host.address.domain.public.add': async params => {
      await pauseFor(2000)

      const patch: Operation<any>[] = [
        {
          op: PatchOp.ADD,
          path: `/serverInfo/network/host/publicDomains`,
          value: {
            [params.fqdn]: { gateway: params.gateway, acme: params.acme },
          },
        },
        {
          op: PatchOp.ADD,
          path: `/serverInfo/network/host/bindings/80/addresses/available/-`,
          value: {
            ssl: true,
            public: true,
            host: params.fqdn,
            port: 443,
            metadata: { kind: 'public-domain', gateway: params.gateway },
          },
        },
      ]
      this.mockRevision(patch)

      return {
        dns: { ipv4: null, ipv6: null },
        port: {
          ip: '0.0.0.0',
          port: 443,
          openExternally: false,
          openInternally: false,
          hairpinning: false,
        },
        portV6: null,
        challenge: null,
      }
    },
    'server.host.address.domain.public.remove': async params => {
      await pauseFor(2000)

      const patch: RemoveOperation[] = [
        {
          op: PatchOp.REMOVE,
          path: `/serverInfo/network/host/publicDomains/${params.fqdn}`,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'server.host.address.domain.private.add': async params => {
      await pauseFor(2000)

      const patch: Operation<any>[] = [
        {
          op: PatchOp.ADD,
          path: `/serverInfo/network/host/privateDomains/${params.fqdn}`,
          value: ['eth0'],
        },
        {
          op: PatchOp.ADD,
          path: `/serverInfo/network/host/bindings/80/addresses/available/-`,
          value: {
            ssl: true,
            public: false,
            host: params.fqdn,
            port: 443,
            metadata: { kind: 'private-domain', gateways: ['eth0'] },
          },
        },
      ]
      this.mockRevision(patch)

      return false
    },
    'server.host.address.domain.private.remove': async params => {
      await pauseFor(2000)

      const patch: RemoveOperation[] = [
        {
          op: PatchOp.REMOVE,
          path: `/serverInfo/network/host/privateDomains/${params.fqdn}`,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'package.host.binding.set-address-enabled': async params => {
      await pauseFor(2000)

      const basePath = `${this.mockHostPath(params.package, params.host)}/bindings/${params.internalPort}/addresses`
      this.mockSetAddressEnabled(basePath, params.address, params.enabled)

      return null
    },
    'package.host.binding.set-range-address-enabled': async params => {
      await pauseFor(2000)

      const basePath = `${this.mockHostPath(params.package, params.host)}/bindingRanges/${params.internalPort}/addresses`
      this.mockSetAddressEnabled(basePath, params.address, params.enabled)

      return null
    },
    'server.host.binding.set-gua-wan': async params => {
      await pauseFor(2000)

      const basePath = `/serverInfo/network/host/bindings/${params.internalPort}/addresses`
      this.mockSetGuaWan(basePath, params.address, params.wan)

      return null
    },
    'package.host.binding.set-gua-wan': async params => {
      await pauseFor(2000)

      const basePath = `${this.mockHostPath(params.package, params.host)}/bindings/${params.internalPort}/addresses`
      this.mockSetGuaWan(basePath, params.address, params.wan)

      return null
    },
    'package.host.address.domain.public.add': async params => {
      await pauseFor(2000)

      const patch: Operation<any>[] = [
        {
          op: PatchOp.ADD,
          path: `${this.mockHostPath(params.package, params.host)}/publicDomains`,
          value: {
            [params.fqdn]: { gateway: params.gateway, acme: params.acme },
          },
        },
        {
          op: PatchOp.ADD,
          path: `${this.mockHostPath(params.package, params.host)}/bindings/80/addresses/available/-`,
          value: {
            ssl: true,
            public: true,
            host: params.fqdn,
            port: 443,
            metadata: { kind: 'public-domain', gateway: params.gateway },
          },
        },
      ]
      this.mockRevision(patch)

      return {
        dns: { ipv4: null, ipv6: null },
        port: {
          ip: '0.0.0.0',
          port: 443,
          openExternally: false,
          openInternally: false,
          hairpinning: false,
        },
        portV6: null,
        challenge: null,
      }
    },
    'package.host.address.domain.public.remove': async params => {
      await pauseFor(2000)

      const patch: RemoveOperation[] = [
        {
          op: PatchOp.REMOVE,
          path: `${this.mockHostPath(params.package, params.host)}/publicDomains/${params.fqdn}`,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'package.host.address.domain.private.add': async params => {
      await pauseFor(2000)

      const patch: Operation<any>[] = [
        {
          op: PatchOp.ADD,
          path: `${this.mockHostPath(params.package, params.host)}/privateDomains/${params.fqdn}`,
          value: ['eth0'],
        },
        {
          op: PatchOp.ADD,
          path: `${this.mockHostPath(params.package, params.host)}/bindings/80/addresses/available/-`,
          value: {
            ssl: true,
            public: false,
            host: params.fqdn,
            port: 443,
            metadata: { kind: 'private-domain', gateways: ['eth0'] },
          },
        },
      ]
      this.mockRevision(patch)

      return false
    },
    'package.host.address.domain.private.remove': async params => {
      await pauseFor(2000)

      const patch: RemoveOperation[] = [
        {
          op: PatchOp.REMOVE,
          path: `${this.mockHostPath(params.package, params.host)}/privateDomains/${params.fqdn}`,
        },
      ]
      this.mockRevision(patch)

      return null
    },
    'registry.package.get': async params =>
      params.id
        ? this.getRegistryPackage(params as GetRegistryPackageReq)
        : this.getRegistryPackages(params as GetRegistryPackagesReq),
    'kiosk.enable': () => this.toggleKiosk(true),
    'kiosk.disable': () => this.toggleKiosk(false),
  }
  readonly rpc = createRpcClient<ApiClient>(tableTransport<Api>(this.handlers))
  readonly streams = createRpcStreamClient<
    ApiService['streams'] & StartupApiService['streams']
  >({
    stream: async (method, params, options) => {
      switch (method) {
        case 'init.logs.follow':
        case 'server.logs.follow':
        case 'server.kernel-logs.follow':
        case 'package.logs.follow':
          return this.logFollowStream(options)
        case 'server.metrics.follow':
          return this.metricsFollowStream(options)
        case 'init.subscribe':
          return this.initSubscriptionStream(options)
        case 'diagnostic.update':
          return this.diagnosticUpdateStream(options)
        default:
          throw new Error(`No mock stream for ${method}`)
      }
    },
  })

  private async metricsFollow(signal?: AbortSignal) {
    await pauseFor(2000, signal)
    return { guid: 'metrics-guid', metrics: Mock.getMetrics() }
  }

  private async metricsFollowStream(options?: RpcStreamOptions) {
    const initial = await this.metricsFollow(options?.signal)
    options?.signal?.throwIfAborted()
    return {
      initial: { ...initial, guid: null },
      items: this.metricsEntries(options?.signal),
    }
  }

  private async *metricsEntries(
    signal?: AbortSignal,
  ): AsyncGenerator<T.Metrics, null> {
    while (true) {
      await pauseFor(1000, signal)
      yield Mock.getMetrics()
    }
  }

  private async logFollow(signal?: AbortSignal) {
    await pauseFor(2000, signal)
    return { startCursor: 'start-cursor', guid: 'logs-guid' }
  }

  private async logFollowStream(options?: RpcStreamOptions) {
    const initial = await this.logFollow(options?.signal)
    options?.signal?.throwIfAborted()
    return {
      initial: { ...initial, guid: null },
      items: this.logEntries(options?.signal),
    }
  }

  private async *logEntries(
    signal?: AbortSignal,
  ): AsyncGenerator<T.LogEntry, null> {
    for (let index = 0; index <= 100; index++) {
      await pauseFor(50, signal)
      if (index === 100) throw new Error('Mock log stream interrupted')
      yield Mock.ServerLogs[0]!
    }
    return null
  }

  async uploadFile(
    guid: string,
    body: Blob,
    options: FileUploadOptions = {},
  ): Promise<void> {
    for (let step = 1; step <= 20; step++) {
      options.signal?.throwIfAborted()
      await pauseFor(100)
      options.progress?.(Math.floor((body.size * step) / 20), body.size)
    }
    options.signal?.throwIfAborted()
  }

  downloadFileUrl(guid: string): string {
    return (
      'data:text/plain;charset=utf-8,' +
      encodeURIComponent('StartOS action file download\n')
    )
  }

  async getStatic(
    urls: string[],
    params: Record<string, string | number>,
  ): Promise<string> {
    await pauseFor(2000)
    return markdown
  }

  async getStaticObjectUrl(url: string): Promise<string> {
    await pauseFor(500)
    const id = url.match(/([^/]+?)(\.s9pk)?\/icon(\?|$)/)?.[1] || ''
    const icons: Record<string, string> = {
      bitcoind: BTC_ICON,
      lnd: LND_ICON,
      'btc-rpc-proxy': PROXY_ICON,
    }

    return icons[id] || FALLBACK_ICON
  }

  openWebsocket$<T>(
    guid: string,
    config: WebsocketConfig<T> = {},
  ): Observable<T> {
    if (guid === 'db-guid') {
      return this.mockWsSource$.pipe<any>(
        shareReplay({ bufferSize: 1, refCount: true }),
      ) as WebSocketSubject<T>
    } else if (guid === 'logs-guid') {
      return withAbortSignal(signal => this.logEntries(signal)).pipe(
        map((entry, index) => {
          if (index === 0) config.openObserver?.next(new Event(''))
          return entry
        }),
      ) as Observable<T>
    } else if (guid === 'init-progress-guid') {
      return from(this.initProgress()).pipe(
        startWith(PROGRESS),
      ) as WebSocketSubject<T>
    } else if (guid === 'sideload-progress-guid') {
      config.openObserver?.next(new Event(''))
      return from(this.initProgress()).pipe(
        startWith(PROGRESS),
      ) as WebSocketSubject<T>
    } else if (guid === 'update-progress-guid') {
      return from(this.initProgress()).pipe(
        startWith(PROGRESS),
        endWith(null),
        finalize(() =>
          config.closeObserver?.next(new CloseEvent('close', { code: 1000 })),
        ),
      ) as WebSocketSubject<T>
    } else if (guid === 'metrics-guid') {
      return withAbortSignal(signal =>
        this.metricsEntries(signal),
      ) as Observable<T>
    } else {
      throw new Error('invalid guid type')
    }
  }

  // state

  async echo(params: Params<'echo'>, url: string): Promise<string> {
    if (url) {
      const num = Math.floor(Math.random() * 10) + 1
      if (num > 8) return params.message
      throw new Error()
    }
    await pauseFor(2000)
    return params.message
  }

  private stateIndex = 0
  // db

  // auth

  // diagnostic

  async getError(): Promise<RPCErrorDetails> {
    await pauseFor(1000)
    return {
      code: 15,
      message: 'Unknown server',
      data: { details: 'Some details about the error here' },
    }
  }

  // init

  // server

  private randomLogs(limit = 1): T.LogEntry[] {
    const arrLength = Math.ceil(limit / Mock.ServerLogs.length)
    const logs = new Array(arrLength)
      .fill(Mock.ServerLogs)
      .reduce((acc, val) => acc.concat(val), [])

    return logs
  }

  async toggleKiosk(enable: boolean): Promise<null> {
    await pauseFor(2000)

    this.mockRevision([
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/kiosk',
        value: enable,
      },
    ])
    this.mockRevision([
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/statusInfo/restart',
        value: 'kiosk',
      },
    ])

    return null
  }

  // marketplace URLs

  // notification

  // proxies

  private proxyId = 0
  // wifi

  // smtp

  // ssh

  // backup

  // async addBackupTarget(
  //   type: BackupTargetType,
  //   params:
  //     | RR.AddCifsBackupTargetReq
  //     | RR.AddCloudBackupTargetReq
  //     | RR.AddDiskBackupTargetReq,
  // ): Promise<RR.AddBackupTargetRes> {
  //   await pauseFor(2000)
  //   const { path, name } = params
  //   return {
  //     latfgvwdbhjsndmk: {
  //       name,
  //       type: 'cifs',
  //       hostname: 'mockhotname',
  //       path: path.replace(/\\/g, '/'),
  //       username: 'mockusername',
  //       mountable: true,
  //       startOs: {},
  //     },
  //   }
  // }

  // async updateBackupTarget(
  //   type: BackupTargetType,
  //   params: RR.UpdateCifsBackupTargetReq | RR.UpdateCloudBackupTargetReq,
  // ): Promise<RR.UpdateBackupTargetRes> {
  //   await pauseFor(2000)
  //   return { [params.id]: Mock.BackupTargets.saved[params.id] }
  // }

  // async removeBackupTarget(
  //   params: RR.RemoveBackupTargetReq,
  // ): Promise<RR.RemoveBackupTargetRes> {
  //   await pauseFor(2000)
  //   return null
  // }

  // async getBackupJobs(
  //   params: RR.GetBackupJobsReq,
  // ): Promise<RR.GetBackupJobsRes> {
  //   await pauseFor(2000)
  //   return Mock.BackupJobs
  // }

  // async createBackupJob(
  //   params: RR.CreateBackupJobReq,
  // ): Promise<RR.CreateBackupJobRes> {
  //   await pauseFor(2000)
  //   return {
  //     id: 'hjdfbjsahdbn',
  //     name: params.name,
  //     targetId: Object.keys(Mock.BackupTargets.saved)[0],
  //     cron: params.cron,
  //     packageIds: params.packageIds,
  //   }
  // }

  // async updateBackupJob(
  //   params: RR.UpdateBackupJobReq,
  // ): Promise<RR.UpdateBackupJobRes> {
  //   await pauseFor(2000)
  //   return {
  //     ...Mock.BackupJobs[0],
  //     ...params,
  //   }
  // }

  // async deleteBackupJob(
  //   params: RR.DeleteBackupJobReq,
  // ): Promise<RR.DeleteBackupJobRes> {
  //   await pauseFor(2000)
  //   return null
  // }

  // async getBackupRuns(
  //   params: RR.GetBackupRunsReq,
  // ): Promise<RR.GetBackupRunsRes> {
  //   await pauseFor(2000)
  //   return Mock.BackupRuns
  // }

  // async deleteBackupRuns(
  //   params: RR.DeleteBackupRunsReq,
  // ): Promise<RR.DeleteBackupRunsRes> {
  //   await pauseFor(2000)
  //   return null
  // }

  // package

  // async setServiceOutboundProxy(
  //   params: RR.SetServiceOutboundTunnelReq,
  // ): Promise<RR.SetServiceOutboundTunnelRes> {
  //   await pauseFor(2000)
  //   const patch = [
  //     {
  //       op: PatchOp.REPLACE,
  //       path: `/packageData/${params.packageId}/outboundProxy`,
  //       value: params.proxy,
  //     },
  //   ]
  //   this.mockRevision(patch)

  //   return null
  // }

  // `start-os` is the server: its host lives in serverInfo, not packageData
  private mockHostPath(pkg: string, host: string): string {
    return pkg === 'start-os'
      ? '/serverInfo/network/host'
      : `/packageData/${pkg}/hosts/${host}`
  }

  private async diagnosticUpdateStream(
    options?: import('@start9labs/start-core').RpcStreamOptions,
  ) {
    options?.signal?.throwIfAborted()
    const initial = await this.diagnosticUpdate(options?.signal)
    options?.signal?.throwIfAborted()
    const progress = () => this.initializationProgress(options?.signal)
    return {
      initial: { ...initial, progress: null },
      items: (async function* () {
        if (initial.target) {
          yield* progress()
        }
        options?.signal?.throwIfAborted()
        return null
      })(),
    }
  }

  private async initSubscription(
    signal?: AbortSignal,
  ): Promise<Result<'init.subscribe'>> {
    await pauseFor(250, signal)
    return { progress: PROGRESS, guid: 'init-progress-guid' }
  }

  private async initSubscriptionStream(
    options?: import('@start9labs/start-core').RpcStreamOptions,
  ) {
    const initial = await this.initSubscription(options?.signal)
    options?.signal?.throwIfAborted()
    return {
      initial: { ...initial, guid: null },
      items: this.initializationProgress(options?.signal),
    }
  }

  private async *initializationProgress(signal?: AbortSignal) {
    signal?.throwIfAborted()
    yield PROGRESS
    yield await this.initProgress(signal)
    signal?.throwIfAborted()
    return null
  }

  private async diagnosticUpdate(
    signal?: AbortSignal,
  ): Promise<Result<'diagnostic.update'>> {
    await pauseFor(1000, signal)
    return { target: '0.4.2', progress: 'update-progress-guid' }
  }

  private async initProgress(signal?: AbortSignal): Promise<T.FullProgress> {
    // PROGRESS contains leaf phase shapes.
    const progress: any = JSON.parse(JSON.stringify(PROGRESS))

    for (let [i, phase] of progress.phases.entries() as [number, any][]) {
      if (
        !phase.progress ||
        typeof phase.progress !== 'object' ||
        !phase.progress.total
      ) {
        await pauseFor(2000, signal)

        progress.phases[i]!.progress = true

        if (
          progress.overall &&
          typeof progress.overall === 'object' &&
          progress.overall.total
        ) {
          const step = progress.overall.total / progress.phases.length
          progress.overall.done += step
        }
      } else {
        const step = phase.progress.total / 4

        while (phase.progress.done < phase.progress.total) {
          await pauseFor(200, signal)

          phase.progress.done += step

          if (
            progress.overall &&
            typeof progress.overall === 'object' &&
            progress.overall.total
          ) {
            const step = progress.overall.total / progress.phases.length / 4

            progress.overall.done += step
          }

          if (phase.progress.done === phase.progress.total) {
            await pauseFor(250, signal)

            progress.phases[i]!.progress = true
          }
        }
      }
    }
    return progress
  }

  private async installProgress(
    id: string,
    finalManifest?: T.Manifest,
  ): Promise<void> {
    // Cast to any: the finalization phase becomes a nested FullProgress at
    // runtime (a service's setInitProgress, injected by installInitProgress).
    const progress: any = JSON.parse(JSON.stringify(PROGRESS))

    for (let [i, phase] of progress.phases.entries() as [number, any][]) {
      // The last phase is finalization ("Installing"): the service reports its
      // own progress via setInitProgress, which the host nests here — but only
      // now, once download/validation are done. Inject it then, and animate it.
      if (i === progress.phases.length - 1) {
        await this.installInitProgress(id, i)

        if (
          progress.overall &&
          typeof progress.overall === 'object' &&
          progress.overall.total
        ) {
          progress.overall.done +=
            progress.overall.total / progress.phases.length

          this.mockRevision([
            {
              op: PatchOp.REPLACE,
              path: `/packageData/${id}/stateInfo/installingInfo/progress/overall/done`,
              value: progress.overall.done,
            },
          ])
        }
      } else if (
        !phase.progress ||
        phase.progress === true ||
        !phase.progress.total
      ) {
        await pauseFor(2000)

        const patches: Operation<any>[] = [
          {
            op: PatchOp.REPLACE,
            path: `/packageData/${id}/stateInfo/installingInfo/progress/phases/${i}/progress`,
            value: true,
          },
        ]

        // overall
        if (
          progress.overall &&
          typeof progress.overall === 'object' &&
          progress.overall.total
        ) {
          const step = progress.overall.total / progress.phases.length

          progress.overall.done += step

          patches.push({
            op: PatchOp.REPLACE,
            path: `/packageData/${id}/stateInfo/installingInfo/progress/overall/done`,
            value: progress.overall.done,
          })
        }

        this.mockRevision(patches)
      } else {
        const step = phase.progress.total / 4

        while (phase.progress.done < phase.progress.total) {
          await pauseFor(500)

          phase.progress.done += step

          const patches: Operation<any>[] = [
            {
              op: PatchOp.REPLACE,
              path: `/packageData/${id}/stateInfo/installingInfo/progress/phases/${i}/progress/done`,
              value: phase.progress.done,
            },
          ]

          // overall
          if (
            progress.overall &&
            typeof progress.overall === 'object' &&
            progress.overall.total
          ) {
            const step = progress.overall.total / progress.phases.length / 4

            progress.overall.done += step

            patches.push({
              op: PatchOp.REPLACE,
              path: `/packageData/${id}/stateInfo/installingInfo/progress/overall/done`,
              value: progress.overall.done,
            })
          }

          this.mockRevision(patches)

          if (phase.progress.done === phase.progress.total) {
            await pauseFor(250)
            this.mockRevision([
              {
                op: PatchOp.REPLACE,
                path: `/packageData/${id}/stateInfo/installingInfo/progress/phases/${i}/progress`,
                value: true,
              },
            ])
          }
        }
      }
    }

    await pauseFor(1000)
    this.mockRevision([
      {
        op: PatchOp.REPLACE,
        path: `/packageData/${id}/stateInfo/installingInfo/progress/overall`,
        value: true,
      },
    ])

    await pauseFor(1000)
    const patch2: Operation<StateInfo>[] = [
      {
        op: PatchOp.REPLACE,
        path: `/packageData/${id}/stateInfo`,
        value: {
          state: 'installed',
          manifest: finalManifest ?? Mock.LocalPkgs[id]?.stateInfo.manifest!,
        },
      },
    ]
    this.mockRevision(patch2)
  }

  // Drives a service's nested init progress at the install's finalization
  // ("Installing") phase. Injects INIT_PROGRESS in place of the not-started
  // (null) phase — every sub-phase starting "waiting" — then runs them all at
  // once (sub-phases can execute concurrently): an indeterminate sub-phase sits
  // "in progress" for a beat, a determinate one climbs its percentage, and each
  // completion advances the nested overall (the parent row's bar). Mirrors the
  // host: the sub-phases appear only now, not earlier.
  private async installInitProgress(
    id: string,
    parentIdx: number,
  ): Promise<void> {
    const base = `/packageData/${id}/stateInfo/installingInfo/progress/phases/${parentIdx}/progress`
    const init: any = JSON.parse(JSON.stringify(INIT_PROGRESS))
    const overall = init.overall

    this.mockRevision([
      {
        op: PatchOp.REPLACE,
        path: base,
        value: {
          overall,
          phases: init.phases.map((p: any) => ({ ...p, progress: null })),
        },
      },
    ])

    await Promise.all(
      init.phases.map(async (sub: any, j: number) => {
        // start this sub-phase (it has been showing "waiting")
        this.mockRevision([
          {
            op: PatchOp.REPLACE,
            path: `${base}/phases/${j}/progress`,
            value: sub.progress,
          },
        ])

        if (
          sub.progress &&
          typeof sub.progress === 'object' &&
          sub.progress.total
        ) {
          const step = sub.progress.total / 4

          while (sub.progress.done < sub.progress.total) {
            await pauseFor(600)
            sub.progress.done += step
            this.mockRevision([
              {
                op: PatchOp.REPLACE,
                path: `${base}/phases/${j}/progress/done`,
                value: sub.progress.done,
              },
            ])
          }
        } else {
          // indeterminate (false) — let the value-less bar animate before completing
          await pauseFor(3000)
        }

        this.mockRevision([
          {
            op: PatchOp.REPLACE,
            path: `${base}/phases/${j}/progress`,
            value: true,
          },
        ])

        // advance the nested overall as each sub-phase finishes
        if (overall && typeof overall === 'object' && overall.total) {
          overall.done += overall.total / init.phases.length
          this.mockRevision([
            {
              op: PatchOp.REPLACE,
              path: `${base}/overall/done`,
              value: overall.done,
            },
          ])
        }
      }),
    )

    this.mockRevision([
      {
        op: PatchOp.REPLACE,
        path: `${base}/overall`,
        value: true,
      },
    ])
  }

  private async updateOSProgress() {
    let size = 10000
    let downloaded = 0

    const patch0 = [
      {
        op: PatchOp.REPLACE,
        path: `/serverInfo/statusInfo/updateProgress/size`,
        value: size,
      },
    ]
    this.mockRevision(patch0)

    while (downloaded < size) {
      await pauseFor(250)
      downloaded += 500
      const patch = [
        {
          op: PatchOp.REPLACE,
          path: `/serverInfo/statusInfo/updateProgress/downloaded`,
          value: downloaded,
        },
      ]
      this.mockRevision(patch)
    }

    const patch2 = [
      {
        op: PatchOp.REPLACE,
        path: `/serverInfo/statusInfo/updateProgress/downloaded`,
        value: size,
      },
    ]
    this.mockRevision(patch2)

    setTimeout(async () => {
      const patch3: Operation<string>[] = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/statusInfo/restart',
          value: 'update',
        },
        {
          op: PatchOp.REMOVE,
          path: '/serverInfo/statusInfo/updateProgress',
        },
      ]
      this.mockRevision(patch3)
      // quickly revert server to "running" for continued testing
      await pauseFor(100)
      const patch4 = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/status',
          value: 'running',
        },
      ]
      this.mockRevision(patch4)
      // set patch indicating update is complete
      await pauseFor(100)
      const patch6 = [
        {
          op: PatchOp.REPLACE,
          path: '/serverInfo/statusInfo',
          value: Mock.ServerUpdated,
        },
      ]
      this.mockRevision(patch6)
    }, 1000)
  }

  private mockSetAddressEnabled(
    basePath: string,
    h: T.HostnameInfoInput,
    enabled: Params<'server.host.binding.set-address-enabled'>['enabled'],
  ): void {
    const isPublicIp =
      h.public && (h.metadata.kind === 'ipv4' || h.metadata.kind === 'ipv6')

    const current = this.mockData(basePath) as T.DerivedAddressInfo

    if (isPublicIp) {
      if (h.port === null) return
      const sa =
        h.metadata.kind === 'ipv6'
          ? `[${h.hostname}]:${h.port}`
          : `${h.hostname}:${h.port}`

      const arr = [...current.enabled]

      if (enabled) {
        if (!arr.includes(sa)) arr.push(sa)
      } else {
        const idx = arr.indexOf(sa)
        if (idx >= 0) arr.splice(idx, 1)
      }

      current.enabled = arr
      this.mockRevision([
        { op: PatchOp.REPLACE, path: `${basePath}/enabled`, value: arr },
      ])
    } else {
      const port = h.port ?? 0
      const without = (overrides: [string, number][]) =>
        overrides.filter(
          ([dHost, dPort]) => !(dHost === h.hostname && dPort === port),
        )
      const disabled = without(current.disabled)
      let lanEnabled = without(current.lanEnabled)

      if (!enabled) {
        disabled.push([h.hostname, port])
        // On a non-SSL port, off takes the LAN IPs it resolves to with it.
        if (h.metadata.kind === 'mdns' && !h.ssl) {
          const gateways = h.metadata.gateways
          lanEnabled = lanEnabled.filter(
            ([host, p]) =>
              !current.available.some(
                ip =>
                  !ip.public &&
                  (ip.metadata.kind === 'ipv4' ||
                    ip.metadata.kind === 'ipv6') &&
                  gateways.includes(ip.metadata.gateway) &&
                  ip.hostname === host &&
                  (ip.port ?? 0) === p,
              ),
          )
        }
      } else if (h.metadata.kind === 'ipv4' || h.metadata.kind === 'ipv6') {
        lanEnabled.push([h.hostname, port])
      }

      current.disabled = disabled
      current.lanEnabled = lanEnabled
      this.mockRevision([
        { op: PatchOp.REPLACE, path: `${basePath}/disabled`, value: disabled },
        {
          op: PatchOp.REPLACE,
          path: `${basePath}/lanEnabled`,
          value: lanEnabled,
        },
      ])
    }
  }

  // Mirrors the backend set_gua_wan: update the gua_wan set, carry the row's
  // on/off state into `enabled`, and re-project `public` onto the available
  // entry (what update_addresses does server-side).
  private mockSetGuaWan(
    basePath: string,
    h: T.HostnameInfoInput,
    wan: boolean,
  ): void {
    if (h.metadata.kind !== 'ipv6' || h.port === null) return

    const key = `[${h.hostname}]:${h.port}`
    const current = this.mockData(basePath) as T.DerivedAddressInfo
    const guaWan = current.guaWan.filter(k => k !== key)
    let enabled = current.enabled.filter(k => k !== key)
    if (wan) {
      guaWan.push(key)
      const off = current.disabled.some(
        ([hostname, port]) => hostname === h.hostname && port === h.port,
      )
      if (!off) enabled = [...enabled, key]
    }
    const available = current.available.map(a =>
      a.hostname === h.hostname && a.port === h.port
        ? { ...a, public: wan }
        : a,
    )

    current.guaWan = guaWan
    current.enabled = enabled
    current.available = available
    const patch: Operation<any>[] = [
      { op: PatchOp.ADD, path: `${basePath}/guaWan`, value: guaWan },
      { op: PatchOp.ADD, path: `${basePath}/enabled`, value: enabled },
      { op: PatchOp.ADD, path: `${basePath}/available`, value: available },
    ]
    this.mockRevision(patch)
  }

  private mockDnsServers(servers: string[] | null): string[] | null {
    if (servers?.length === 0)
      throw new Error('At least one DNS server is required')
    return (
      servers?.map(server => {
        if (new RegExp(utils.regexes.ipv4.matches()).test(server))
          return server + ':53'
        if (new RegExp(utils.regexes.ipv6.matches()).test(server))
          return '[' + server + ']:53'
        return server
      }) ?? null
    )
  }

  private mockGatewayDns(id: string, servers: string[] | null): T.DnsSettings {
    const gateway = this.mockData('/serverInfo/network/gateways/' + id) as
      | T.NetworkInterfaceInfo
      | undefined
    if (!gateway) throw new Error('Unknown gateway')
    const staticServers = this.mockDnsServers(servers)
    const provided = gateway.ipInfo?.dnsServers ?? []
    return {
      staticServers,
      servers:
        staticServers ??
        this.mockDnsServers(
          provided.length ? provided : ['1.1.1.1', '2606:4700:4700::1111'],
        )!,
      source: staticServers
        ? 'static'
        : provided.length
          ? 'gateway'
          : 'public-fallback',
    }
  }

  private offeredKeys(url: string): string[] {
    return Object.entries(mockPatchData.serverInfo.registryKeys)
      .filter(([saved]) => sameUrl(saved, url))
      .flatMap(([, keys]) => keys)
  }

  private keepOsKeys(ids: readonly string[]): void {
    const keys = Object.entries(mockPatchData.serverInfo.trustedKeys)
    const removed = new Set(ids)
    if (
      keys.some(([, key]) => key.os) &&
      !keys.some(([id, key]) => !removed.has(id) && key.os)
    )
      throw new Error('At least one key must stay trusted for StartOS updates')
  }

  private async removeKeys(ids: readonly string[]): Promise<null> {
    await pauseFor(2000)
    const keys = structuredClone(mockPatchData.serverInfo.trustedKeys)
    this.protectTrust(keys)
    for (const id of ids) {
      if (!keys[id]) throw new Error('Key not found')
      if (!keys[id].removable) throw new Error('Protected key')
    }
    this.keepOsKeys(ids)
    for (const id of ids) delete keys[id]
    mockPatchData.serverInfo.trustedKeys = keys
    await this.publishTrust()
    return null
  }

  private importRegistryKeys(
    url: string,
    info: T.RegistryInfo,
  ): { added: string[] } {
    const proposed = registrySigningKeys(info)
    const added: string[] = []
    for (const [id, identity] of Object.entries(proposed)) {
      const trusted = mockPatchData.serverInfo.trustedKeys[id]
      if (trusted) {
        trusted.contact = [
          ...new Set([...trusted.contact, ...identity.contact]),
        ]
        if (!trusted.gpgExports.length) trusted.gpgExports = identity.gpgExports
        continue
      }
      added.push(id)
      mockPatchData.serverInfo.trustedKeys[id] = {
        ...identity,
        origin: url.replace(/\/$/, ''),
        originRegistry: { name: info.name, icon: info.icon },
        addedAt: new Date().toISOString(),
        removable: false,
        marketplace: false,
        os: false,
      }
    }
    for (const saved of Object.keys(mockPatchData.serverInfo.registryKeys)) {
      if (sameUrl(saved, url))
        delete mockPatchData.serverInfo.registryKeys[saved]
    }
    mockPatchData.serverInfo.registryKeys[url] = Object.keys(proposed)
    return { added }
  }

  private mockRegistryInfo(url: string): T.RegistryInfo {
    return {
      ...Mock.RegistryInfo,
      name:
        Object.entries(mockPatchData.serverInfo.registries).find(([saved]) =>
          sameUrl(saved, url),
        )?.[1] || 'Example Registry',
    }
  }

  private protectTrust(keys = mockPatchData.serverInfo.trustedKeys): void {
    const listed = new Set(
      Object.entries(mockPatchData.serverInfo.registryKeys)
        .filter(([url]) =>
          Object.keys(mockPatchData.serverInfo.registries).some(saved =>
            sameUrl(saved, url),
          ),
        )
        .flatMap(([, ids]) => ids.map(auth.normalizeDer64)),
    )
    for (const key of Object.values(keys)) {
      key.removable = !listed.has(auth.normalizeDer64(key.publicKey))
    }
  }

  private async publishTrust(): Promise<void> {
    this.protectTrust()
    await this.mockRevision<unknown>([
      {
        op: PatchOp.REPLACE,
        path: '/registryCache',
        value: structuredClone(mockPatchData.registryCache),
      },
      {
        op: PatchOp.REPLACE,
        path: '/osUpdateCache',
        value: structuredClone(mockPatchData.osUpdateCache),
      },
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/trustedKeys',
        value: structuredClone(mockPatchData.serverInfo.trustedKeys),
      },
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/registryKeys',
        value: structuredClone(mockPatchData.serverInfo.registryKeys),
      },
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/registries',
        value: structuredClone(mockPatchData.serverInfo.registries),
      },
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/osRegistry',
        value: mockPatchData.serverInfo.osRegistry,
      },
    ])
  }

  private async publishConversions(): Promise<void> {
    await this.mockRevision<T.ServerStatus['backupConversions']>([
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/statusInfo/backupConversions',
        value: structuredClone(this.conversionJobs),
      },
    ])
  }

  private backupTarget(id: string): BackupTargetEntry | undefined {
    return this.mockData('/backupTargets')[id]
  }

  private cifsTarget(id: string) {
    const target = this.backupTarget(id)
    if (target?.source.kind !== 'cifs') throw new Error('CIFS target not found')
    return target
  }

  private mockSource(
    source: Params<'volumes.create'>['source'],
  ): T.SharedVolumeSource {
    switch (source.kind) {
      case 'cifs':
        return {
          kind: 'cifs',
          hostname: source.hostname,
          path: `/${source.path.replace(/^\/+/, '')}`,
          username: source.username ?? '',
        }
      case 'disk':
        return Mock.diskSource(source.logicalname, source.path)
      default:
        return source
    }
  }

  private async publishBackup(
    { targetId, serverId }: { targetId: string; serverId: string },
    backup: T.StartOsRecoveryInfo,
  ): Promise<void> {
    await this.mockRevision<unknown>([
      {
        op: PatchOp.REPLACE,
        path: `/backupTargets/${targetId}/startOs/${serverId}`,
        value: structuredClone(backup),
      },
    ])
  }

  private queueSourceCheck(collection: SourceCollection, id: string) {
    void pauseFor(1000).then(() => {
      if (this.mockData(`/${collection}`)[id])
        return this.checkSource(collection, id)
    })
  }

  private async checkSource(
    collection: SourceCollection,
    id: string,
  ): Promise<Result<'volumes.inspect'>> {
    const volume: { source: T.SharedVolumeSource } | undefined = this.mockData(
      `/${collection}`,
    )[id]
    if (!volume) throw new Error('Not found')
    const prefix =
      'The volume source is unavailable. Check the source, network and credentials.: '
    const { source } = volume
    const result: Omit<Result<'volumes.inspect'>, 'checkedAt'> = source.kind ===
    'local'
      ? { availability: 'available', available: 812400000000, error: null }
      : source.kind === 'disk'
        ? this.attachedDrives.has(source.logicalname)
          ? {
              availability: 'available',
              available: this.attachedDrives.get(source.logicalname)!,
              error: null,
            }
          : {
              availability: 'unavailable',
              available: null,
              error: `${prefix}mount: special device ${source.logicalname} does not exist`,
            }
        : source.hostname.endsWith('.invalid')
          ? {
              availability: 'unavailable',
              available: null,
              error: `${prefix}mount error(113): could not connect to ${source.hostname}`,
            }
          : {
              availability: 'available',
              available: 3215000000000,
              error: null,
            }
    const inspection = { ...result, checkedAt: new Date().toISOString() }
    const partition =
      collection === 'backupTargets' && source.kind === 'disk' && !source.path
        ? Mock.Disks.flatMap(disk => disk.partitions).find(
            partition =>
              Mock.driveLink(partition.logicalname) === source.logicalname,
          )
        : undefined
    await this.mockRevision<unknown>(
      Object.entries({
        ...inspection,
        ...(partition && result.availability === 'available'
          ? { startOs: partition.startOs, legacyBackup: partition.legacyBackup }
          : {}),
      }).map(([key, value]) => ({
        op: PatchOp.REPLACE,
        path: `/${collection}/${id}/${key}`,
        value,
      })),
    )
    return inspection
  }

  private async publishDisks(): Promise<void> {
    await this.mockRevision<unknown>([
      {
        op: PatchOp.REPLACE,
        path: '/disks',
        value: structuredClone(Mock.Disks),
      },
    ])
  }

  private guid(): string {
    return Array.from(
      this.crypto.getRandomValues(new Uint8Array(32)),
      value => 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'[value & 31],
    ).join('')
  }

  private async simulateConversion(
    params: Params<'backup.target.convert'>,
    backup: T.StartOsRecoveryInfo,
  ): Promise<void> {
    const job = this.conversionJobs[params.targetId]![params.serverId]!
    const progress = new utils.FullProgressTracker()
    const phases = [
      ['Inspecting backup', 'steps', 5],
      ['Converting metadata', 'bytes', 20],
      ['Scanning backup objects', 'steps', 10],
      ['Converting data objects', 'steps', 60],
      ['Finalizing conversion', 'steps', 5],
    ] as const
    const handles = phases.map(([name, , weight]) =>
      progress.addPhase(name, weight),
    )
    for (const [index, [name, units]] of phases.entries()) {
      const phase = handles[index]!
      phase.setUnits(units)
      phase.setTotal(100)
      for (const done of [0, 25, 50, 75, 100]) {
        phase.setDone(done)
        job.progress = progress.snapshot()
        if (
          name === 'Converting data objects' &&
          done === 25 &&
          params.password === 'interrupt'
        ) {
          job.state = 'failed'
          job.error =
            'Conversion interrupted. Resume it when the target is available.'
          await this.publishConversions()
          return
        }
        await this.publishConversions()
        await pauseFor(700)
      }
      phase.complete()
    }
    backup.formatStatus = 'current'
    await this.publishBackup(params, backup)
    job.state = 'complete'
    progress.complete()
    job.progress = progress.snapshot()
    await this.publishConversions()
  }

  private mockData(path: string): any {
    const parts = path.split('/').filter(Boolean)
    let obj: any = this.cache.value.value
    for (const part of parts) {
      obj = obj[part]
    }
    return obj
  }

  private async mockRevision<T>(patch: Operation<T>[]): Promise<void> {
    const revision = {
      id: ++this.sequence,
      patch,
    }
    this.mockWsSource$.next(revision)
  }

  private async getRegistryPackage(params: GetRegistryPackageReq) {
    await pauseFor(2000)

    const { targetVersion, sourceVersion, id } = params

    if (sourceVersion) {
      // Mock registry refinement: return a reachable stepping-stone version
      if (id === 'bitcoind') {
        return Mock.OtherPackageVersions['bitcoind']!['=26.1.0:0.1.0']!
      }
      return Mock.RegistryPackages[id]!
    }

    if (!targetVersion || targetVersion === '=*') {
      return Mock.RegistryPackages[id]!
    } else {
      return Mock.OtherPackageVersions[id]![targetVersion]!
    }
  }
  private async getRegistryPackages(params: GetRegistryPackagesReq) {
    await pauseFor(2000)
    return Mock.RegistryPackages
  }

  private async setNut(params: Params<'server.set-nut'>): Promise<null> {
    await pauseFor(2000)
    const settings = params.config.settings ?? null
    const config: T.NutConfig = {
      enabled: params.config.enabled ?? false,
      settings: !settings
        ? null
        : settings.mode === 'client'
          ? { ...settings, port: settings.port ?? 3493 }
          : {
              ...settings,
              device: {
                ...settings.device,
                vendorId: settings.device.vendorId ?? null,
                productId: settings.device.productId ?? null,
                serial: settings.device.serial ?? null,
                bus: settings.device.bus ?? null,
                busPort: settings.device.busPort ?? null,
                vendor: settings.device.vendor ?? null,
                product: settings.device.product ?? null,
              },
              listenAll: settings.listenAll ?? false,
              remoteUsername: settings.remoteUsername ?? null,
              remotePassword: settings.remotePassword ?? null,
            },
    }
    mockPatchData.serverInfo.nut = config
    this.mockRevision([
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/nut',
        value: config,
      },
    ])
    return null
  }

  private async getNutStatus(params: {}): Promise<T.NutStatus> {
    await pauseFor(1000)
    const settings: T.NutSettings | null = this.mockData(
      '/serverInfo/nut/settings',
    )
    const host = settings?.mode === 'client' ? settings.host : 'localhost'
    const port = settings?.mode === 'client' ? settings.port : 3493
    return {
      target: `${settings?.mode === 'client' ? settings.upsName : 'startos'}@${host.includes(':') && !host.startsWith('[') ? `[${host}]` : host}:${port}`,
      monitoringError: null,
      variables: {
        'battery.charge': '96',
        'battery.charge.low': '10',
        'battery.charge.warning': '20',
        'battery.mfr.date': '2023/04/11',
        'battery.runtime': '4280',
        'battery.runtime.low': '300',
        'battery.type': 'PbAc',
        'battery.voltage': '13.5',
        'battery.voltage.nominal': '12',
        'device.mfr': 'Start9 Mock UPS',
        'device.model': 'NUT Simulator',
        'device.serial': 'MOCK-UPS-001',
        'device.type': 'ups',
        'driver.name': 'usbhid-ups',
        'driver.parameter.pollinterval': '15',
        'driver.parameter.port': 'auto',
        'driver.version': '2.8.1',
        'driver.version.data': 'CyberPower HID 0.8',
        'input.transfer.high': '139',
        'input.transfer.low': '88',
        'input.voltage': '120.1',
        'input.voltage.nominal': '120',
        'output.voltage': '120.0',
        'ups.beeper.status': 'enabled',
        'ups.delay.shutdown': '20',
        'ups.load': '18',
        'ups.mfr': 'Start9 Mock UPS',
        'ups.model': 'NUT Simulator',
        'ups.productid': '0501',
        'ups.realpower.nominal': '900',
        'ups.serial': 'MOCK-UPS-001',
        'ups.status': 'OL CHRG',
        'ups.test.result': 'No test initiated',
        'ups.timer.shutdown': '-1',
        'ups.timer.start': '-1',
        'ups.vendorid': '0764',
      },
    }
  }
}
