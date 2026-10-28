import type { Api, ApiClient, Params, Result } from './api.types'
import { inject, Injectable } from '@angular/core'
import { WA_SESSION_STORAGE } from '@ng-web-apis/common'
import {
  FALLBACK_ICON,
  GetPackageRes,
  GetPackagesRes,
} from '@start9labs/marketplace'
import {
  FullKeyboard,
  pauseFor,
  RPCErrorDetails,
  SetLanguageParams,
} from '@start9labs/shared'
import {
  createRpcClient,
  RpcHandlers,
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
  shareReplay,
  startWith,
  Subject,
} from 'rxjs'
import { WebSocketSubject } from 'rxjs/webSocket'
import {
  DataModel,
  InstallingState,
  PackageDataEntry,
  StateInfo,
  UIData,
  UpdatingState,
} from 'src/app/services/patch-db/data-model'
import { PATCH_CACHE } from 'src/app/services/patch-db/patch-db-source'
import { toAuthorityUrl } from 'src/app/utils/acme'
import { Mock } from './api.fixures'
import { BTC_ICON, LND_ICON, PROXY_ICON } from './api-icons'
import {
  ActionRes,
  CheckDnsRes,
  CifsBackupTarget,
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
import { ApiService } from './embassy-api.service'
import { mockPatchData } from './mock-patch'

import markdown from './md-sample.md'

const UI_KEY = '_startos/mock-ui'

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

// A service's own install-time progress (setInitProgress). The host nests this
// inside the install's finalization ("Installing") phase as a FullProgress — but
// only once finalization begins; until then that phase is null (waiting), so the
// sub-phases must not appear before the download/validation phases finish.
// installProgress injects this then. An indeterminate sub-phase (like HA's
// "Generating Home Assistant configuration", a value-less animated bar) plus a
// determinate one exercise both render branches of the install card's sub-phases.
const INIT_PROGRESS: T.FullProgress = {
  overall: { done: 0, total: 8, units: 'steps' },
  phases: [
    { name: 'Generating configuration', progress: false },
    { name: 'Migrating data', progress: { done: 0, total: 8, units: 'steps' } },
  ],
}

@Injectable()
export class MockApiService extends ApiService {
  readonly mockWsSource$ = new Subject<Revision>()
  private readonly storage = inject(WA_SESSION_STORAGE)
  private readonly cache = inject(PATCH_CACHE)
  private readonly revertTime = 1800
  sequence = 1
  private readonly handlers: RpcHandlers<Api> = {
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
    'diagnostic.update': async () => {
      await pauseFor(1000)
      return { target: '0.4.1', progress: 'update-progress-guid' }
    },
    'diagnostic.logs': async params => {
      return this.handlers['server.logs']!(params)
    },
    'init.subscribe': async () => {
      await pauseFor(250)
      return {
        progress: PROGRESS,
        guid: 'init-progress-guid',
      }
    },
    'init.logs.follow': async () => {
      await pauseFor(2000)
      return {
        startCursor: 'start-cursor',
        guid: 'logs-guid',
      }
    },
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
    'server.logs.follow': async params => {
      await pauseFor(2000)
      return {
        startCursor: 'start-cursor',
        guid: 'logs-guid',
      }
    },
    'server.kernel-logs.follow': async params => {
      await pauseFor(2000)
      return {
        startCursor: 'start-cursor',
        guid: 'logs-guid',
      }
    },
    'server.metrics.follow': async params => {
      await pauseFor(2000)
      return {
        guid: 'metrics-guid',
        metrics: Mock.getMetrics(),
      }
    },
    'server.device-info': async () => ({
      os: {
        version: mockPatchData.serverInfo.version,
        compat: mockPatchData.serverInfo.packageVersionCompat,
        platform: 'x86_64',
        language: mockPatchData.serverInfo.language,
      },
      hardware: null,
    }),
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
    'disk.list': async () => {
      await pauseFor(1000)
      return Mock.Disks
    },
    'disk.format': async ({ disk }) => {
      await pauseFor(3000)
      return `${disk}1`
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
      return Mock.RegistryOSUpdate as Result<'registry.os.version.get'>
    },
    'registry.info': async params => {
      await pauseFor(2000)
      return Mock.RegistryInfo
    },
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
    'backup.target.list': async params => {
      await pauseFor(2000)
      return Mock.BackupTargets
    },
    'backup.target.cifs.add': async params => {
      await pauseFor(2000)
      const { hostname, path, username } = params
      return {
        latfgvwdbhjsndmk: {
          type: 'cifs',
          hostname,
          path,
          username,
          mountable: true,
          available: 50000000000,
          startOs: {},
          legacyBackup: false,
        },
      }
    },
    'backup.target.cifs.update': async params => {
      await pauseFor(2000)
      const { id, hostname, path, username } = params
      return {
        [id]: {
          ...(Mock.BackupTargets[id] as CifsBackupTarget),
          hostname,
          path,
          username,
        },
      }
    },
    'backup.target.cifs.remove': async params => {
      await pauseFor(2000)
      return null
    },
    'backup.target.delete-legacy': async params => {
      await pauseFor(2000)
      const target = Mock.BackupTargets[params.targetId]
      if (target) target.legacyBackup = false
      return null
    },
    'backup.target.info': async params => {
      await pauseFor(2000)
      return Mock.BackupInfo
    },
    'backup.create': async params => {
      await pauseFor(2000)
      const serverPath = '/serverInfo/statusInfo/backupProgress'
      const ids = params.packageIds || []
      // One phase per package plus a trailing "OS Data" phase (the host
      // writes os-backup.json + LUKS folder after the per-package loop).
      const phaseCount = ids.length + 1

      setTimeout(async () => {
        for (let i = 0; i < ids.length; i++) {
          const id = ids[i]!
          const appPath = `/packageData/${id}/statusInfo/desired/main`
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
              value: 'stopped',
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
        }

        // OS Data phase
        const osIdx = ids.length
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

        const lastPatch: ReplaceOperation<T.ServerStatus['backupProgress']>[] =
          [
            {
              op: PatchOp.REPLACE,
              path: serverPath,
              value: null,
            },
          ]
        this.mockRevision(lastPatch)

        // Feature 1: a completed backup whose target still holds a legacy (V1)
        // folder raises a warning notification — bumps the unread badge and
        // lands in the notifications list, pointing the user at the create page.
        if (Mock.BackupTargets[params.targetId]?.legacyBackup) {
          const count = mockPatchData.serverInfo.unreadNotificationCount + 1
          mockPatchData.serverInfo.unreadNotificationCount = count
          Mock.Notifications.unshift({
            id: Math.max(0, ...Mock.Notifications.map(n => n.id)) + 1,
            packageId: null,
            createdAt: new Date().toISOString(),
            code: 0,
            level: 'warning',
            title: 'Old Backup Still Present',
            message:
              'This backup target still contains an old-format (V1) backup that is no longer needed. To free up space on the drive, go to the backup creation page and delete it.',
            data: null,
            seen: false,
          })
          this.mockRevision([
            {
              op: PatchOp.REPLACE,
              path: '/serverInfo/unreadNotificationCount',
              value: count,
            },
          ])
        }
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
    'package.logs.follow': async params => {
      await pauseFor(2000)
      return {
        startCursor: 'start-cursor',
        guid: 'logs-guid',
      }
    },
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

      // return Mock.ActionResGroup
      // return Mock.ActionResSingle
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

  async uploadFile(guid: string, body: Blob): Promise<void> {
    await pauseFor(2000)
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

  // websocket

  openWebsocket$<T>(
    guid: string,
    config: WebsocketConfig<T> = {},
  ): WebSocketSubject<T> {
    if (guid === 'db-guid') {
      return this.mockWsSource$.pipe<any>(
        shareReplay({ bufferSize: 1, refCount: true }),
      ) as WebSocketSubject<T>
    } else if (guid === 'logs-guid') {
      return interval(50).pipe<any>(
        map((_, index) => {
          // mock fire open observer
          if (index === 0) config.openObserver?.next(new Event(''))
          if (index === 100) throw new Error('HAAHHA')
          return Mock.ServerLogs[0]
        }),
      ) as WebSocketSubject<T>
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
      return interval(1000).pipe(
        map(() => Mock.getMetrics()),
      ) as WebSocketSubject<T>
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

  private async initProgress(): Promise<T.FullProgress> {
    // Cast to any: PhaseProgress now allows nested FullProgress, but the mock
    // PROGRESS fixture only uses leaf shapes — we know it at the call site.
    const progress: any = JSON.parse(JSON.stringify(PROGRESS))

    for (let [i, phase] of progress.phases.entries() as [number, any][]) {
      if (
        !phase.progress ||
        typeof phase.progress !== 'object' ||
        !phase.progress.total
      ) {
        await pauseFor(2000)

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
          await pauseFor(200)

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
            await pauseFor(250)

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
}
