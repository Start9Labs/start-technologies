import { DOCUMENT, inject, Injectable } from '@angular/core'
import { blake3 } from '@noble/hashes/blake3'
import { GetPackageRes, GetPackagesRes } from '@start9labs/marketplace'
import {
  FullKeyboard,
  HttpOptions,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
  SetLanguageParams,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { Dump, pathFromArray } from 'patch-db-client'
import { filter, firstValueFrom, Observable } from 'rxjs'
import { webSocket, WebSocketSubject } from 'rxjs/webSocket'
import { PATCH_CACHE } from 'src/app/services/patch-db/patch-db-source'
import { AuthService } from '../auth.service'
import { DataModel } from '../patch-db/data-model'
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

@Injectable()
export class LiveApiService extends ApiService {
  private readonly document = inject(DOCUMENT)
  private readonly http = inject(HttpService)
  private readonly auth = inject(AuthService)
  private readonly cache$ = inject<Observable<Dump<DataModel>>>(PATCH_CACHE)

  constructor() {
    super()

    // @ts-ignore
    this.document.defaultView.rpcClient = this
  }

  // for uploading files

  async uploadFile(guid: string, body: Blob): Promise<void> {
    await this.httpRequest({
      method: 'POST',
      body,
      url: `/rest/rpc/${guid}`,
      timeout: 0,
    })
  }

  // for getting static files: ex: license

  async getStatic(
    urls: string[],
    params: Record<string, string | number>,
  ): Promise<string> {
    for (const url of urls) {
      try {
        const res = await this.httpRequest<string>({
          method: 'GET',
          url,
          params,
          responseType: 'text',
        })
        return res
      } catch (e) {}
    }
    throw new Error('Could not fetch static file')
  }

  // websocket

  openWebsocket$<T>(
    guid: string,
    config: WebsocketConfig<T> = {},
  ): WebSocketSubject<T> {
    const { location } = this.document.defaultView!
    const protocol = location.protocol === 'http:' ? 'ws' : 'wss'
    const host = location.host

    return webSocket({
      url: `${protocol}://${host}/ws/rpc/${guid}`,
      ...config,
    })
  }

  // state

  async echo(params: T.EchoParams, url: string): Promise<string> {
    return this.rpcRequest({ method: 'echo', params }, url)
  }

  async getState(): Promise<ServerState> {
    return this.rpcRequest({ method: 'state', params: {}, timeout: 10000 })
  }

  // db

  async subscribeToPatchDB(params: {}): Promise<{
    dump: Dump<DataModel>
    guid: string
  }> {
    return this.rpcRequest({ method: 'db.subscribe', params })
  }

  async setDbValue<T>(
    pathArr: Array<string | number>,
    value: T,
  ): Promise<null> {
    const pointer = pathFromArray(pathArr)
    const params = { pointer, value }
    return this.rpcRequest({ method: 'db.put.ui', params })
  }

  // auth

  async login(params: T.LoginParams): Promise<null> {
    return this.rpcRequest({ method: 'auth.login', params })
  }

  async logout(params: {}): Promise<null> {
    return this.rpcRequest({ method: 'auth.logout', params })
  }

  async getSessions(params: {}): Promise<T.SessionList> {
    return this.rpcRequest({ method: 'auth.session.list', params })
  }

  async killSessions(params: T.KillParams): Promise<null> {
    return this.rpcRequest({ method: 'auth.session.kill', params })
  }

  async resetPassword(params: T.ResetPasswordParams): Promise<null> {
    return this.rpcRequest({ method: 'auth.reset-password', params })
  }

  // diagnostic

  async diagnosticGetError(): Promise<DiagnosticErrorRes> {
    return this.rpcRequest<DiagnosticErrorRes>({
      method: 'diagnostic.error',
      params: {},
    })
  }

  async diagnosticRestart(): Promise<void> {
    return this.rpcRequest<void>({
      method: 'diagnostic.restart',
      params: {},
    })
  }

  async diagnosticForgetDrive(): Promise<void> {
    return this.rpcRequest<void>({
      method: 'diagnostic.disk.forget',
      params: {},
    })
  }

  async diagnosticRepairDisk(): Promise<void> {
    return this.rpcRequest<void>({
      method: 'diagnostic.disk.repair',
      params: {},
    })
  }

  async diagnosticGetLogs(params: T.LogsParams): Promise<T.LogResponse> {
    return this.rpcRequest<T.LogResponse>({
      method: 'diagnostic.logs',
      params,
    })
  }

  // init

  async initFollowProgress(): Promise<T.SetupProgress> {
    return this.rpcRequest({ method: 'init.subscribe', params: {} })
  }

  async initFollowLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse> {
    return this.rpcRequest({ method: 'init.logs.follow', params })
  }

  // server

  async getSystemTime(params: {}): Promise<T.TimeInfo> {
    return this.rpcRequest({ method: 'server.time', params })
  }

  async getServerLogs(params: T.LogsParams): Promise<T.LogResponse> {
    return this.rpcRequest({ method: 'server.logs', params })
  }

  async getKernelLogs(params: T.LogsParams): Promise<T.LogResponse> {
    return this.rpcRequest({ method: 'server.kernel-logs', params })
  }

  async followServerLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse> {
    return this.rpcRequest({ method: 'server.logs.follow', params })
  }

  async followKernelLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse> {
    return this.rpcRequest({ method: 'server.kernel-logs.follow', params })
  }

  async followServerMetrics(params: {}): Promise<T.MetricsFollowResponse> {
    return this.rpcRequest({ method: 'server.metrics.follow', params })
  }

  async updateServer(params: {
    registry: string
    targetVersion: string
  }): Promise<'updating' | 'no-updates'> {
    return this.rpcRequest({ method: 'server.update', params })
  }

  async restartServer(params: {}): Promise<null> {
    return this.rpcRequest({ method: 'server.restart', params })
  }

  async shutdownServer(params: {}): Promise<null> {
    return this.rpcRequest({ method: 'server.shutdown', params })
  }

  async repairDisk(params: {}): Promise<null> {
    return this.rpcRequest({ method: 'disk.repair', params })
  }

  async toggleKiosk(enable: boolean): Promise<null> {
    return this.rpcRequest({
      method: enable ? 'kiosk.enable' : 'kiosk.disable',
      params: {},
    })
  }

  async setHostname(params: T.SetServerHostnameParams): Promise<null> {
    return this.rpcRequest({ method: 'server.set-hostname', params })
  }

  async setKeyboard(params: FullKeyboard): Promise<null> {
    return this.rpcRequest({ method: 'server.set-keyboard', params })
  }

  async setLanguage(params: SetLanguageParams): Promise<null> {
    return this.rpcRequest({ method: 'server.set-language', params })
  }

  async setDns(params: T.SetStaticDnsParams): Promise<null> {
    return this.rpcRequest({
      method: 'net.dns.set-static',
      params,
    })
  }

  async queryDns(params: T.QueryDnsParams): Promise<string | null> {
    return this.rpcRequest({
      method: 'net.dns.query',
      params,
    })
  }

  async checkPort(params: T.CheckPortParams): Promise<T.CheckPortRes> {
    return this.rpcRequest({
      method: 'net.gateway.check-port',
      params,
    })
  }

  async checkDns(params: T.CheckDnsParams): Promise<CheckDnsRes> {
    return this.rpcRequest({
      method: 'net.gateway.check-dns',
      params,
    })
  }

  // marketplace URLs

  async checkOSUpdate(params: {
    registry: string
    serverId: string
  }): Promise<T.OsVersionInfoMap> {
    return this.rpcRequest({
      method: 'registry.os.version.get',
      params,
    })
  }

  async getRegistryInfo(params: { registry: string }): Promise<T.RegistryInfo> {
    return this.rpcRequest({
      method: 'registry.info',
      params,
    })
  }

  async getRegistryPackage(
    params: GetRegistryPackageReq,
  ): Promise<GetPackageRes> {
    return this.rpcRequest({
      method: 'registry.package.get',
      params,
    })
  }

  async getRegistryPackages(
    params: GetRegistryPackagesReq,
  ): Promise<GetPackagesRes> {
    return this.rpcRequest({
      method: 'registry.package.get',
      params,
    })
  }

  // notification

  async getNotifications(
    params: T.ListNotificationParams,
  ): Promise<T.NotificationWithId[]> {
    return this.rpcRequest({ method: 'notification.list', params })
  }

  async deleteNotifications(params: T.ModifyNotificationParams): Promise<null> {
    return this.rpcRequest({ method: 'notification.remove', params })
  }

  async markSeenNotifications(
    params: T.ModifyNotificationParams,
  ): Promise<null> {
    return this.rpcRequest({ method: 'notification.mark-seen', params })
  }

  async markSeenAllNotifications(
    params: T.ModifyNotificationBeforeParams,
  ): Promise<null> {
    return this.rpcRequest({
      method: 'notification.mark-seen-before',
      params,
    })
  }

  async markUnseenNotifications(
    params: T.ModifyNotificationParams,
  ): Promise<null> {
    return this.rpcRequest({ method: 'notification.mark-unseen', params })
  }

  // proxies

  async addTunnel(params: T.AddTunnelParams): Promise<{ id: string }> {
    return this.rpcRequest({ method: 'net.tunnel.add', params })
  }

  async updateTunnel(params: T.RenameGatewayParams): Promise<null> {
    return this.rpcRequest({ method: 'net.gateway.set-name', params })
  }

  async updateTunnelConfig(params: T.UpdateTunnelParams): Promise<null> {
    return this.rpcRequest({ method: 'net.tunnel.update', params })
  }

  async removeTunnel(params: T.RemoveTunnelParams): Promise<null> {
    return this.rpcRequest({ method: 'net.tunnel.remove', params })
  }

  async setDefaultOutbound(params: T.SetDefaultOutboundParams): Promise<null> {
    return this.rpcRequest({
      method: 'net.gateway.set-default-outbound',
      params,
    })
  }

  async setServiceOutbound(params: T.SetOutboundGatewayParams): Promise<null> {
    return this.rpcRequest({ method: 'package.set-outbound-gateway', params })
  }

  // wifi

  async enableWifi(params: T.SetWifiEnabledParams): Promise<null> {
    return this.rpcRequest({ method: 'wifi.set-enabled', params })
  }

  async getWifi(params: {}, timeout?: number): Promise<T.WifiListInfo> {
    return this.rpcRequest({ method: 'wifi.get', params, timeout })
  }

  async setWifiCountry(params: T.SetCountryParams): Promise<null> {
    return this.rpcRequest({ method: 'wifi.country.set', params })
  }

  async addWifi(params: T.WifiAddParams): Promise<null> {
    return this.rpcRequest({ method: 'wifi.add', params })
  }

  async connectWifi(params: T.WifiSsidParams): Promise<null> {
    return this.rpcRequest({ method: 'wifi.connect', params })
  }

  async deleteWifi(params: T.WifiSsidParams): Promise<null> {
    return this.rpcRequest({ method: 'wifi.remove', params })
  }

  // smtp

  async setSmtp(params: T.SmtpValue): Promise<null> {
    return this.rpcRequest({ method: 'server.set-smtp', params })
  }

  async clearSmtp(params: {}): Promise<null> {
    return this.rpcRequest({ method: 'server.clear-smtp', params })
  }

  async testSmtp(params: T.TestSmtpParams): Promise<null> {
    return this.rpcRequest({ method: 'server.test-smtp', params })
  }

  // ssh

  async getSshKeys(params: {}): Promise<T.SshKeyResponse[]> {
    return this.rpcRequest({ method: 'ssh.list', params })
  }

  async addSshKey(params: T.SshAddParams): Promise<T.SshKeyResponse> {
    return this.rpcRequest({ method: 'ssh.add', params })
  }

  async deleteSshKey(params: T.SshDeleteParams): Promise<null> {
    return this.rpcRequest({ method: 'ssh.remove', params })
  }

  // backup

  async getBackupTargets(params: {}): Promise<{
    [id: string]: T.BackupTarget
  }> {
    return this.rpcRequest({ method: 'backup.target.list', params })
  }

  async addBackupTarget(
    params: T.CifsAddParams,
  ): Promise<{ [id: string]: CifsBackupTarget }> {
    params.path = params.path.replace('/\\/g', '/')
    return this.rpcRequest({ method: 'backup.target.cifs.add', params })
  }

  async updateBackupTarget(
    params: T.CifsUpdateParams,
  ): Promise<{ [id: string]: CifsBackupTarget }> {
    return this.rpcRequest({ method: 'backup.target.cifs.update', params })
  }

  async removeBackupTarget(params: T.CifsRemoveParams): Promise<null> {
    return this.rpcRequest({ method: 'backup.target.cifs.remove', params })
  }

  async deleteLegacyBackup(params: T.DeleteLegacyParams): Promise<null> {
    return this.rpcRequest({ method: 'backup.target.delete-legacy', params })
  }

  async getBackupInfo(params: T.InfoParams): Promise<T.BackupInfo> {
    return this.rpcRequest({ method: 'backup.target.info', params })
  }

  async createBackup(params: T.BackupParams): Promise<null> {
    return this.rpcRequest({ method: 'backup.create', params })
  }

  // async addBackupTarget(
  //   type: BackupTargetType,
  //   params: RR.AddCifsBackupTargetReq | RR.AddCloudBackupTargetReq,
  // ): Promise<RR.AddBackupTargetRes> {
  //   params.path = params.path.replace('/\\/g', '/')
  //   return this.rpcRequest({ method: `backup.target.${type}.add`, params })
  // }

  // async updateBackupTarget(
  //   type: BackupTargetType,
  //   params: RR.UpdateCifsBackupTargetReq | RR.UpdateCloudBackupTargetReq,
  // ): Promise<RR.UpdateBackupTargetRes> {
  //   return this.rpcRequest({ method: `backup.target.${type}.update`, params })
  // }

  // async removeBackupTarget(
  //   params: RR.RemoveBackupTargetReq,
  // ): Promise<RR.RemoveBackupTargetRes> {
  //   return this.rpcRequest({ method: 'backup.target.remove', params })
  // }

  // async getBackupJobs(
  //   params: RR.GetBackupJobsReq,
  // ): Promise<RR.GetBackupJobsRes> {
  //   return this.rpcRequest({ method: 'backup.job.list', params })
  // }

  // async createBackupJob(
  //   params: RR.CreateBackupJobReq,
  // ): Promise<RR.CreateBackupJobRes> {
  //   return this.rpcRequest({ method: 'backup.job.create', params })
  // }

  // async updateBackupJob(
  //   params: RR.UpdateBackupJobReq,
  // ): Promise<RR.UpdateBackupJobRes> {
  //   return this.rpcRequest({ method: 'backup.job.update', params })
  // }

  // async deleteBackupJob(
  //   params: RR.DeleteBackupJobReq,
  // ): Promise<RR.DeleteBackupJobRes> {
  //   return this.rpcRequest({ method: 'backup.job.delete', params })
  // }

  // async getBackupRuns(
  //   params: RR.GetBackupRunsReq,
  // ): Promise<RR.GetBackupRunsRes> {
  //   return this.rpcRequest({ method: 'backup.runs.list', params })
  // }

  // async deleteBackupRuns(
  //   params: RR.DeleteBackupRunsReq,
  // ): Promise<RR.DeleteBackupRunsRes> {
  //   return this.rpcRequest({ method: 'backup.runs.delete', params })
  // }

  // package

  async getPackageLogs(params: GetPackageLogsReq): Promise<T.LogResponse> {
    return this.rpcRequest({ method: 'package.logs', params })
  }

  async followPackageLogs(
    params: FollowPackageLogsReq,
  ): Promise<T.LogFollowResponse> {
    return this.rpcRequest({ method: 'package.logs.follow', params })
  }

  async installPackage(params: T.InstallParams): Promise<null> {
    return this.rpcRequest({ method: 'package.install', params })
  }

  async cancelInstallPackage(params: T.CancelInstallParams): Promise<null> {
    return this.rpcRequest({ method: 'package.cancel-install', params })
  }

  async getActionInput(
    params: T.GetActionInputParams,
  ): Promise<GetActionInputRes> {
    return this.rpcRequest({ method: 'package.action.get-input', params })
  }

  async runAction(params: T.RunActionParams): Promise<ActionRes> {
    return this.rpcRequest({ method: 'package.action.run', params })
  }

  async clearTask(params: T.ClearTaskParams): Promise<null> {
    return this.rpcRequest({ method: 'package.action.clear-task', params })
  }

  async restorePackages(params: T.RestorePackageParams): Promise<null> {
    return this.rpcRequest({ method: 'package.backup.restore', params })
  }

  async startPackage(params: T.ControlParams): Promise<null> {
    return this.rpcRequest({ method: 'package.start', params })
  }

  async restartPackage(params: T.ControlParams): Promise<null> {
    return this.rpcRequest({ method: 'package.restart', params })
  }

  async stopPackage(params: T.ControlParams): Promise<null> {
    return this.rpcRequest({ method: 'package.stop', params })
  }

  async rebuildPackage(params: T.RebuildParams): Promise<null> {
    return this.rpcRequest({ method: 'package.rebuild', params })
  }

  async uninstallPackage(params: T.UninstallParams): Promise<null> {
    return this.rpcRequest({ method: 'package.uninstall', params })
  }

  async sideloadPackage(): Promise<T.SideloadResponse> {
    return this.rpcRequest({
      method: 'package.sideload',
      params: {},
    })
  }

  // async setServiceOutboundProxy(
  //   params: RR.SetServiceOutboundTunnelReq,
  // ): Promise<RR.SetServiceOutboundTunnelRes> {
  //   return this.rpcRequest({ method: 'package.proxy.set-outbound', params })
  // }

  async removeAcme(params: T.RemoveAcmeParams): Promise<null> {
    return this.rpcRequest({
      method: 'net.acme.remove',
      params,
    })
  }

  async initAcme(params: T.InitAcmeParams): Promise<null> {
    return this.rpcRequest({
      method: 'net.acme.init',
      params,
    })
  }

  async serverBindingSetAddressEnabled(
    params: ServerBindingSetAddressEnabledReq,
  ): Promise<null> {
    return this.rpcRequest({
      method: 'server.host.binding.set-address-enabled',
      params,
    })
  }

  async osUiAddPublicDomain(
    params: T.AddPublicDomainParams,
  ): Promise<T.AddPublicDomainRes> {
    return this.rpcRequest({
      method: 'server.host.address.domain.public.add',
      params,
    })
  }

  async osUiRemovePublicDomain(params: T.RemoveDomainParams): Promise<null> {
    return this.rpcRequest({
      method: 'server.host.address.domain.public.remove',
      params,
    })
  }

  async osUiAddPrivateDomain(
    params: T.AddPrivateDomainParams,
  ): Promise<boolean> {
    return this.rpcRequest({
      method: 'server.host.address.domain.private.add',
      params,
    })
  }

  async osUiRemovePrivateDomain(params: T.RemoveDomainParams): Promise<null> {
    return this.rpcRequest({
      method: 'server.host.address.domain.private.remove',
      params,
    })
  }

  async pkgBindingSetAddressEnabled(
    params: PkgBindingSetAddressEnabledReq,
  ): Promise<null> {
    return this.rpcRequest({
      method: 'package.host.binding.set-address-enabled',
      params,
    })
  }

  async pkgBindingSetRangeAddressEnabled(
    params: PkgBindingSetAddressEnabledReq,
  ): Promise<null> {
    return this.rpcRequest({
      method: 'package.host.binding.set-range-address-enabled',
      params,
    })
  }

  async serverBindingSetGuaWan(
    params: ServerBindingSetGuaWanReq,
  ): Promise<null> {
    return this.rpcRequest({
      method: 'server.host.binding.set-gua-wan',
      params,
    })
  }

  async pkgBindingSetGuaWan(params: PkgBindingSetGuaWanReq): Promise<null> {
    return this.rpcRequest({
      method: 'package.host.binding.set-gua-wan',
      params,
    })
  }

  async pkgAddPublicDomain(
    params: PkgAddPublicDomainReq,
  ): Promise<T.AddPublicDomainRes> {
    return this.rpcRequest({
      method: 'package.host.address.domain.public.add',
      params,
    })
  }

  async pkgRemovePublicDomain(params: PkgRemovePublicDomainReq): Promise<null> {
    return this.rpcRequest({
      method: 'package.host.address.domain.public.remove',
      params,
    })
  }

  async pkgAddPrivateDomain(params: PkgAddPrivateDomainReq): Promise<boolean> {
    return this.rpcRequest({
      method: 'package.host.address.domain.private.add',
      params,
    })
  }

  async pkgRemovePrivateDomain(
    params: PkgRemovePrivateDomainReq,
  ): Promise<null> {
    return this.rpcRequest({
      method: 'package.host.address.domain.private.remove',
      params,
    })
  }

  private async rpcRequest<T>(
    options: RPCOptions,
    urlOverride?: string,
  ): Promise<T> {
    const res = await this.http.rpcRequest<T>(options, urlOverride)
    const body = res.body

    if (isRpcError(body)) {
      if (body.error.code === 34) {
        console.error('Unauthenticated, logging out')
        this.auth.setUnverified()
      }
      throw new RpcError(body.error)
    }

    const patchSequence = res.headers.get('x-patch-sequence')
    if (patchSequence)
      await firstValueFrom(
        this.cache$.pipe(filter(({ id }) => id >= Number(patchSequence))),
      )

    return body.result
  }

  private async httpRequest<T>(opts: HttpOptions): Promise<T> {
    const res = await this.http.httpRequest<T>(opts)
    if (res.headers.get('Repr-Digest')) {
      // verify
      const digest = res.headers.get('Repr-Digest')!
      let data: Uint8Array
      if (opts.responseType === 'arrayBuffer') {
        data = Buffer.from(res.body as ArrayBuffer)
      } else if (opts.responseType === 'text') {
        data = Buffer.from(res.body as string)
      } else if ((opts.responseType as string) === 'blob') {
        data = Buffer.from(await (res.body as Blob).arrayBuffer())
      } else {
        console.warn(
          `could not verify Repr-Digest for responseType ${
            opts.responseType || 'json'
          }`,
        )
        return res.body
      }
      const [alg, hash] = digest.split('=', 2)
      if (alg === 'blake3') {
        if (
          Buffer.from(blake3(data)).compare(
            Buffer.from(hash?.replace(/:/g, '') || '', 'base64'),
          ) !== 0
        ) {
          throw new Error('File digest mismatch.')
        }
      } else {
        console.warn(`Unknown Repr-Digest algorithm ${alg}`)
      }
    }
    return res.body
  }
}
