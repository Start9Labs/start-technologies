import { inject, Injectable } from '@angular/core'
import { GetPackageRes, GetPackagesRes } from '@start9labs/marketplace'
import {
  FullKeyboard,
  pauseFor,
  RPCErrorDetails,
  SetLanguageParams,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  AddOperation,
  Dump,
  Operation,
  PatchOp,
  pathFromArray,
  RemoveOperation,
  ReplaceOperation,
  Revision,
} from 'patch-db-client'
import { from, interval, map, shareReplay, startWith, Subject, tap } from 'rxjs'
import { WebSocketSubject } from 'rxjs/webSocket'
import {
  DataModel,
  InstallingState,
  PackageDataEntry,
  StateInfo,
  UpdatingState,
} from 'src/app/services/patch-db/data-model'
import { toAuthorityUrl } from 'src/app/utils/acme'
import { AuthService } from '../auth.service'
import { Mock } from './api.fixures'
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
  private readonly revertTime = 1800
  sequence = 0

  constructor() {
    super()
    inject(AuthService)
      .isVerified$.pipe(tap(() => (this.sequence = 0)))
      .subscribe()
  }

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
    } else if (guid === 'metrics-guid') {
      return interval(1000).pipe(
        map(() => Mock.getMetrics()),
      ) as WebSocketSubject<T>
    } else {
      throw new Error('invalid guid type')
    }
  }

  // state

  async echo(params: T.EchoParams, url: string): Promise<string> {
    if (url) {
      const num = Math.floor(Math.random() * 10) + 1
      if (num > 8) return params.message
      throw new Error()
    }
    await pauseFor(2000)
    return params.message
  }

  private stateIndex = 0
  async getState(): Promise<ServerState> {
    await pauseFor(1000)

    this.stateIndex++

    return this.stateIndex === 1 ? 'running' : 'running'
  }

  // db

  async subscribeToPatchDB(params: {}): Promise<{
    dump: Dump<DataModel>
    guid: string
  }> {
    await pauseFor(2000)
    return {
      dump: { id: 1, value: mockPatchData },
      guid: 'db-guid',
    }
  }

  async setDbValue<T>(
    pathArr: Array<string | number>,
    value: T,
  ): Promise<null> {
    const pointer = pathFromArray(pathArr)
    const params = { pointer, value }
    await pauseFor(2000)
    const patch = [
      {
        op: PatchOp.REPLACE,
        path: '/ui' + params.pointer,
        value: params.value,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  // auth

  async login(params: T.LoginParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async logout(params: {}): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async getSessions(params: {}): Promise<T.SessionList> {
    await pauseFor(2000)
    return Mock.Sessions
  }

  async killSessions(params: T.KillParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async resetPassword(params: T.ResetPasswordParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  // diagnostic

  async getError(): Promise<RPCErrorDetails> {
    await pauseFor(1000)
    return {
      code: 15,
      message: 'Unknown server',
      data: { details: 'Some details about the error here' },
    }
  }

  async diagnosticGetError(): Promise<DiagnosticErrorRes> {
    await pauseFor(1000)
    return {
      code: 15,
      message: 'Unknown server',
      data: { details: 'Some details about the error here' },
    }
  }

  async diagnosticRestart(): Promise<void> {
    await pauseFor(1000)
  }

  async diagnosticForgetDrive(): Promise<void> {
    await pauseFor(1000)
  }

  async diagnosticRepairDisk(): Promise<void> {
    await pauseFor(1000)
  }

  async diagnosticGetLogs(params: T.LogsParams): Promise<T.LogResponse> {
    return this.getServerLogs(params)
  }

  // init

  async initFollowProgress(): Promise<T.SetupProgress> {
    await pauseFor(250)
    return {
      progress: PROGRESS,
      guid: 'init-progress-guid',
    }
  }

  async initFollowLogs(): Promise<T.LogFollowResponse> {
    await pauseFor(2000)
    return {
      startCursor: 'start-cursor',
      guid: 'logs-guid',
    }
  }

  // server

  async getSystemTime(params: {}): Promise<T.TimeInfo> {
    await pauseFor(2000)
    return {
      now: new Date().toUTCString(),
      uptime: 1234567n,
    }
  }

  async getServerLogs(params: T.LogsParams): Promise<T.LogResponse> {
    await pauseFor(2000)
    const entries = this.randomLogs(params.limit)

    return {
      entries,
      startCursor: 'start-cursor',
      endCursor: 'end-cursor',
    }
  }

  async getKernelLogs(params: T.LogsParams): Promise<T.LogResponse> {
    await pauseFor(2000)
    const entries = this.randomLogs(params.limit)

    return {
      entries,
      startCursor: 'start-cursor',
      endCursor: 'end-cursor',
    }
  }

  async followServerLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse> {
    await pauseFor(2000)
    return {
      startCursor: 'start-cursor',
      guid: 'logs-guid',
    }
  }

  async followKernelLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse> {
    await pauseFor(2000)
    return {
      startCursor: 'start-cursor',
      guid: 'logs-guid',
    }
  }

  private randomLogs(limit = 1): T.LogEntry[] {
    const arrLength = Math.ceil(limit / Mock.ServerLogs.length)
    const logs = new Array(arrLength)
      .fill(Mock.ServerLogs)
      .reduce((acc, val) => acc.concat(val), [])

    return logs
  }

  async followServerMetrics(params: {}): Promise<T.MetricsFollowResponse> {
    await pauseFor(2000)
    return {
      guid: 'metrics-guid',
      metrics: Mock.getMetrics(),
    }
  }

  async updateServer(params?: {
    registry: string
    targetVersion: string
  }): Promise<'updating' | 'no-updates'> {
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

    return 'updating'
  }

  async restartServer(params: {}): Promise<null> {
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
  }

  async shutdownServer(params: {}): Promise<null> {
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
  }

  async repairDisk(params: {}): Promise<null> {
    await pauseFor(2000)
    return null
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

  async setHostname(params: T.SetServerHostnameParams): Promise<null> {
    await pauseFor(1000)

    this.mockRevision([
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/name',
        value: params.name,
      },
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
  }

  async setKeyboard(params: FullKeyboard): Promise<null> {
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
  }

  async setLanguage(params: SetLanguageParams): Promise<null> {
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
  }

  async setDns(params: T.SetStaticDnsParams): Promise<null> {
    await pauseFor(2000)

    const patch: ReplaceOperation<T.DnsSettings['staticServers']>[] = [
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/network/dns/staticServers',
        value: params.servers,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  async queryDns(params: T.QueryDnsParams): Promise<T.QueryDnsRes> {
    await pauseFor(2000)

    return { ipv4: null, ipv6: null }
  }

  async checkPort(params: T.CheckPortParams): Promise<T.CheckPortRes> {
    await pauseFor(2000)

    return {
      ip: '0.0.0.0',
      port: params.port,
      openExternally: true,
      openInternally: false,
      hairpinning: true,
    }
  }

  async checkPortV6(
    params: T.CheckPortParams,
  ): Promise<T.CheckPortV6Res | null> {
    await pauseFor(2000)

    return {
      ip: '::',
      openExternally: true,
      openInternally: false,
    }
  }

  async checkDns(params: T.CheckDnsParams): Promise<CheckDnsRes> {
    await pauseFor(2000)

    return false
  }

  // marketplace URLs

  async checkOSUpdate(params: {
    registry: string
    serverId: string
  }): Promise<T.OsVersionInfoMap> {
    await pauseFor(2000)
    return Mock.RegistryOSUpdate
  }

  async getRegistryInfo(params: { registry: string }): Promise<T.RegistryInfo> {
    await pauseFor(2000)
    return Mock.RegistryInfo
  }

  async getRegistryPackage(
    params: GetRegistryPackageReq,
  ): Promise<GetPackageRes> {
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

  async getRegistryPackages(
    params: GetRegistryPackagesReq,
  ): Promise<GetPackagesRes> {
    await pauseFor(2000)
    return Mock.RegistryPackages
  }

  // notification

  async getNotifications(
    params: T.ListNotificationParams,
  ): Promise<T.NotificationWithId[]> {
    await pauseFor(2000)

    return Mock.Notifications
  }

  async deleteNotifications(params: T.ModifyNotificationParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async markSeenNotifications(
    params: T.ModifyNotificationParams,
  ): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async markSeenAllNotifications(
    params: T.ModifyNotificationBeforeParams,
  ): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async markUnseenNotifications(
    params: T.ModifyNotificationParams,
  ): Promise<null> {
    await pauseFor(2000)
    return null
  }

  // proxies

  private proxyId = 0
  async addTunnel(params: T.AddTunnelParams): Promise<{ id: string }> {
    await pauseFor(2000)

    const id = `wg${this.proxyId++}`

    const patch: AddOperation<any>[] = [
      {
        op: PatchOp.ADD,
        path: `/serverInfo/network/gateways/${id}`,
        value: {
          name: params.name,
          secure: false,
          ipInfo: {
            name: id,
            scopeId: 3,
            deviceType: 'wireguard',
            subnets: ['192.168.1.10/24'],
            wanIp: '203.0.113.45',
            ntpServers: [],
            lanIp: ['192.168.1.10'],
            dnsServers: [],
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

    return { id }
  }

  async updateTunnel(params: T.RenameGatewayParams): Promise<null> {
    await pauseFor(2000)

    const patch: ReplaceOperation<string>[] = [
      {
        op: PatchOp.REPLACE,
        path: `/serverInfo/network/gateways/${params.id}/label`,
        value: params.name,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  async updateTunnelConfig(params: T.UpdateTunnelParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async removeTunnel(params: T.RemoveTunnelParams): Promise<null> {
    await pauseFor(2000)
    const patch: RemoveOperation[] = [
      {
        op: PatchOp.REMOVE,
        path: `/serverInfo/network/gateways/${params.id}`,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  async setDefaultOutbound(params: { gateway: string | null }): Promise<null> {
    await pauseFor(2000)
    const patch = [
      {
        op: PatchOp.REPLACE,
        path: '/serverInfo/network/defaultOutbound',
        value: params.gateway,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  async setServiceOutbound(params: T.SetOutboundGatewayParams): Promise<null> {
    await pauseFor(2000)
    const patch = [
      {
        op: PatchOp.REPLACE,
        path: `/packageData/${params.package}/outboundGateway`,
        value: params.gateway,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  // wifi

  async enableWifi(params: T.SetWifiEnabledParams): Promise<null> {
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
  }

  async setWifiCountry(params: T.SetCountryParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async getWifi(params: {}, timeout: number): Promise<T.WifiListInfo> {
    await pauseFor(2000)
    return Mock.Wifi
  }

  async addWifi(params: T.WifiAddParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async connectWifi(params: T.WifiSsidParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async deleteWifi(params: T.WifiSsidParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  // smtp

  async setSmtp(params: T.SmtpValue): Promise<null> {
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
  }

  async clearSmtp(params: {}): Promise<null> {
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
  }

  async testSmtp(params: T.TestSmtpParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  // ssh

  async getSshKeys(params: {}): Promise<T.SshKeyResponse[]> {
    await pauseFor(2000)
    return Mock.SshKeys
  }

  async addSshKey(params: T.SshAddParams): Promise<T.SshKeyResponse> {
    await pauseFor(2000)
    return Mock.SshKey
  }

  async deleteSshKey(params: T.SshDeleteParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  // backup

  async getBackupTargets(params: {}): Promise<{
    [id: string]: T.BackupTarget
  }> {
    await pauseFor(2000)
    return Mock.BackupTargets
  }

  async addBackupTarget(
    params: T.CifsAddParams,
  ): Promise<{ [id: string]: CifsBackupTarget }> {
    await pauseFor(2000)
    const { hostname, path, username } = params
    return {
      latfgvwdbhjsndmk: {
        type: 'cifs',
        hostname,
        path: path.replace(/\\/g, '/'),
        username,
        mountable: true,
        startOs: {},
        legacyBackup: null,
      },
    }
  }

  async updateBackupTarget(
    params: T.CifsUpdateParams,
  ): Promise<{ [id: string]: CifsBackupTarget }> {
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
  }

  async removeBackupTarget(params: T.CifsRemoveParams): Promise<null> {
    await pauseFor(2000)
    return null
  }

  async deleteLegacyBackup(params: T.DeleteLegacyParams): Promise<null> {
    await pauseFor(2000)
    const target = Mock.BackupTargets[params.targetId]
    if (target) target.legacyBackup = null
    return null
  }

  async getBackupInfo(params: T.InfoParams): Promise<T.BackupInfo> {
    await pauseFor(2000)
    return Mock.BackupInfo
  }

  async createBackup(params: T.BackupParams): Promise<null> {
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

      const lastPatch: ReplaceOperation<T.ServerStatus['backupProgress']>[] = [
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

    const originalPatch: ReplaceOperation<T.ServerStatus['backupProgress']>[] =
      [
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
  }

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

  async getPackageLogs(params: GetPackageLogsReq): Promise<T.LogResponse> {
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
  }

  async followPackageLogs(
    params: FollowPackageLogsReq,
  ): Promise<T.LogFollowResponse> {
    await pauseFor(2000)
    return {
      startCursor: 'start-cursor',
      guid: 'logs-guid',
    }
  }

  async installPackage(params: T.InstallParams): Promise<null> {
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
  }

  async cancelInstallPackage(params: T.CancelInstallParams): Promise<null> {
    await pauseFor(500)

    const patch: RemoveOperation[] = [
      {
        op: PatchOp.REMOVE,
        path: `/packageData/${params.id}`,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  async getActionInput(
    params: T.GetActionInputParams,
  ): Promise<GetActionInputRes> {
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

    return {
      eventId: 'ANZXNWIFRTTBZ6T52KQPZILIQQODDHXQ',
      value: Mock.MockConfig,
      spec: await Mock.getActionInputSpec(),
    }
  }

  async runAction(params: T.RunActionParams): Promise<ActionRes> {
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
    return Mock.ActionResMessage
    // return Mock.ActionResSingle
  }

  async clearTask(params: T.ClearTaskParams): Promise<null> {
    await pauseFor(2000)

    const patch: RemoveOperation[] = [
      {
        op: PatchOp.REMOVE,
        path: `/packageData/${params.packageId}/tasks/${params.replayId}`,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  async restorePackages(params: T.RestorePackageParams): Promise<null> {
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
  }

  async startPackage(params: T.ControlParams): Promise<null> {
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
  }

  async restartPackage(params: T.ControlParams): Promise<null> {
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
  }

  async stopPackage(params: T.ControlParams): Promise<null> {
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
  }

  async rebuildPackage(params: T.RebuildParams): Promise<null> {
    return this.restartPackage(params)
  }

  async uninstallPackage(params: T.UninstallParams): Promise<null> {
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
  }

  async sideloadPackage(): Promise<T.SideloadResponse> {
    await pauseFor(2000)
    return {
      upload: 'sideload-upload-guid', // no significance, randomly generated
      progress: 'sideload-progress-guid', // no significance, randomly generated
    }
  }

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

  async initAcme(params: T.InitAcmeParams): Promise<null> {
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
  }

  async removeAcme(params: T.RemoveAcmeParams): Promise<null> {
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
  }

  async serverBindingSetAddressEnabled(
    params: ServerBindingSetAddressEnabledReq,
  ): Promise<null> {
    await pauseFor(2000)

    const basePath = `/serverInfo/network/host/bindings/${params.internalPort}/addresses`
    this.mockSetAddressEnabled(basePath, params.address, params.enabled)

    return null
  }

  async osUiAddPublicDomain(
    params: T.AddPublicDomainParams,
  ): Promise<T.AddPublicDomainRes> {
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
    }
  }

  async osUiRemovePublicDomain(params: T.RemoveDomainParams): Promise<null> {
    await pauseFor(2000)

    const patch: RemoveOperation[] = [
      {
        op: PatchOp.REMOVE,
        path: `/serverInfo/network/host/publicDomains/${params.fqdn}`,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  async osUiAddPrivateDomain(
    params: T.AddPrivateDomainParams,
  ): Promise<boolean> {
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
  }

  async osUiRemovePrivateDomain(params: T.RemoveDomainParams): Promise<null> {
    await pauseFor(2000)

    const patch: RemoveOperation[] = [
      {
        op: PatchOp.REMOVE,
        path: `/serverInfo/network/host/privateDomains/${params.fqdn}`,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  // `start-os` is the server: its host lives in serverInfo, not packageData
  private mockHostPath(pkg: string, host: string): string {
    return pkg === 'start-os'
      ? '/serverInfo/network/host'
      : `/packageData/${pkg}/hosts/${host}`
  }

  async pkgBindingSetAddressEnabled(
    params: PkgBindingSetAddressEnabledReq,
  ): Promise<null> {
    await pauseFor(2000)

    const basePath = `${this.mockHostPath(params.package, params.host)}/bindings/${params.internalPort}/addresses`
    this.mockSetAddressEnabled(basePath, params.address, params.enabled)

    return null
  }

  async pkgBindingSetRangeAddressEnabled(
    params: PkgBindingSetAddressEnabledReq,
  ): Promise<null> {
    await pauseFor(2000)

    const basePath = `${this.mockHostPath(params.package, params.host)}/bindingRanges/${params.internalPort}/addresses`
    this.mockSetAddressEnabled(basePath, params.address, params.enabled)

    return null
  }

  async serverBindingSetGuaWan(
    params: ServerBindingSetGuaWanReq,
  ): Promise<null> {
    await pauseFor(2000)

    const basePath = `/serverInfo/network/host/bindings/${params.internalPort}/addresses`
    this.mockSetGuaWan(basePath, params.address, params.wan)

    return null
  }

  async pkgBindingSetGuaWan(params: PkgBindingSetGuaWanReq): Promise<null> {
    await pauseFor(2000)

    const basePath = `${this.mockHostPath(params.package, params.host)}/bindings/${params.internalPort}/addresses`
    this.mockSetGuaWan(basePath, params.address, params.wan)

    return null
  }

  async pkgAddPublicDomain(
    params: PkgAddPublicDomainReq,
  ): Promise<T.AddPublicDomainRes> {
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
    }
  }

  async pkgRemovePublicDomain(params: PkgRemovePublicDomainReq): Promise<null> {
    await pauseFor(2000)

    const patch: RemoveOperation[] = [
      {
        op: PatchOp.REMOVE,
        path: `${this.mockHostPath(params.package, params.host)}/publicDomains/${params.fqdn}`,
      },
    ]
    this.mockRevision(patch)

    return null
  }

  async pkgAddPrivateDomain(params: PkgAddPrivateDomainReq): Promise<boolean> {
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
  }

  async pkgRemovePrivateDomain(
    params: PkgRemovePrivateDomainReq,
  ): Promise<null> {
    await pauseFor(2000)

    const patch: RemoveOperation[] = [
      {
        op: PatchOp.REMOVE,
        path: `${this.mockHostPath(params.package, params.host)}/privateDomains/${params.fqdn}`,
      },
    ]
    this.mockRevision(patch)

    return null
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
    h: T.HostnameInfo,
    enabled: boolean | null,
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
      const arr = current.disabled.filter(
        ([dHost, dPort]) => !(dHost === h.hostname && dPort === port),
      )

      if (!enabled) {
        arr.push([h.hostname, port])
      }

      current.disabled = arr
      this.mockRevision([
        { op: PatchOp.REPLACE, path: `${basePath}/disabled`, value: arr },
      ])
    }
  }

  // Mirrors the backend set_gua_wan: update the gua_wan set, carry the row's
  // on/off state into `enabled`, and re-project `public` onto the available
  // entry (what update_addresses does server-side).
  private mockSetGuaWan(
    basePath: string,
    h: T.HostnameInfo,
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

  private mockData(path: string): any {
    const parts = path.split('/').filter(Boolean)
    let obj: any = mockPatchData
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
}
