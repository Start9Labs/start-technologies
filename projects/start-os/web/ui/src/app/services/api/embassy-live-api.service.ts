import type { Api, Params } from './api.types'
import { DOCUMENT, inject, Injectable } from '@angular/core'
import { blake3 } from '@noble/hashes/blake3'
import {
  GetPackageRes,
  GetPackagesRes,
  packageResponse,
  packagesResponse,
} from '@start9labs/marketplace'
import {
  AuthKeyService,
  FullKeyboard,
  HttpOptions,
  HttpService,
  isRpcError,
  RpcError,
  RPCOptions,
  SetLanguageParams,
} from '@start9labs/shared'
import { IST, RPC, T } from '@start9labs/start-core'
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
  private readonly authKeys = inject(AuthKeyService)
  private readonly cache$ = inject<Observable<Dump<DataModel>>>(PATCH_CACHE)

  constructor() {
    super()

    // @ts-ignore
    this.document.defaultView.rpcClient = this
  }

  async uploadFile(guid: string, body: Blob): Promise<void> {
    await this.httpRequest({
      method: 'POST',
      body,
      url: `/rest/rpc/${guid}`,
      // Service-worker fetch events expire during long uploads.
      headers: { 'ngsw-bypass': 'true' },
      timeout: 0,
    })
  }

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

  async echo(params: Params<'echo'>, url: string): Promise<string> {
    return this.rpcRequest({ method: 'echo', params }, url)
  }

  async getState(): Promise<ServerState> {
    return this.rpcRequest({ method: 'state', params: {}, timeout: 10000 })
  }

  // db

  async subscribeToPatchDB(): Promise<{
    dump: Dump<DataModel>
    guid: string
  }> {
    const result = await this.rpcRequest({ method: 'db.subscribe', params: {} })
    // Omitting the pointer subscribes to the complete UI database.
    return {
      ...result,
      dump: { ...result.dump, value: result.dump.value as DataModel },
    }
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

  async login(params: Params<'auth.login'>): Promise<null> {
    return this.rpcRequest({ method: 'auth.login', params })
  }

  async logout(params: {}): Promise<null> {
    return this.rpcRequest({ method: 'auth.logout', params })
  }

  async getSessions(params: {}): Promise<T.SessionList> {
    return this.rpcRequest({ method: 'auth.session.list', params })
  }

  async killSessions(params: Params<'auth.session.kill'>): Promise<null> {
    return this.rpcRequest({ method: 'auth.session.kill', params })
  }

  async resetPassword(params: Params<'auth.reset-password'>): Promise<null> {
    return this.rpcRequest({ method: 'auth.reset-password', params })
  }

  // diagnostic

  async diagnosticGetError(): Promise<DiagnosticErrorRes> {
    return this.rpcRequest({
      method: 'diagnostic.error',
      params: {},
    })
  }

  async diagnosticRestart(): Promise<null> {
    return this.rpcRequest({
      method: 'diagnostic.restart',
      params: {},
    })
  }

  async diagnosticForgetDrive(): Promise<null> {
    return this.rpcRequest({
      method: 'diagnostic.disk.forget',
      params: {},
    })
  }

  async diagnosticRepairDisk(): Promise<null> {
    return this.rpcRequest({
      method: 'diagnostic.disk.repair',
      params: {},
    })
  }

  async diagnosticGetLogs(
    params: Params<'diagnostic.logs'>,
  ): Promise<T.LogResponse> {
    return this.rpcRequest({
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

  async getServerLogs(params: Params<'server.logs'>): Promise<T.LogResponse> {
    return this.rpcRequest({ method: 'server.logs', params })
  }

  async getKernelLogs(
    params: Params<'server.kernel-logs'>,
  ): Promise<T.LogResponse> {
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

  async updateServer(
    params: Params<'server.update'>,
  ): Promise<T.UpdateSystemRes> {
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

  async setHostname(params: Params<'server.set-hostname'>): Promise<null> {
    return this.rpcRequest({ method: 'server.set-hostname', params })
  }

  async setKeyboard(params: FullKeyboard): Promise<null> {
    return this.rpcRequest({ method: 'server.set-keyboard', params })
  }

  async setLanguage(params: SetLanguageParams): Promise<null> {
    return this.rpcRequest({ method: 'server.set-language', params })
  }

  async setDns(params: Params<'net.dns.set-static'>): Promise<null> {
    return this.rpcRequest({
      method: 'net.dns.set-static',
      params,
    })
  }

  async queryDns(params: Params<'net.dns.query'>): Promise<T.QueryDnsRes> {
    return this.rpcRequest({
      method: 'net.dns.query',
      params,
    })
  }

  async checkPort(
    params: Params<'net.gateway.check-port'>,
  ): Promise<T.CheckPortRes> {
    return this.rpcRequest({
      method: 'net.gateway.check-port',
      params,
    })
  }

  async checkPortV6(
    params: Params<'net.gateway.check-port-v6'>,
  ): Promise<T.CheckPortV6Res | null> {
    return this.rpcRequest({
      method: 'net.gateway.check-port-v6',
      params,
    })
  }

  async checkChallenge(
    params: Params<'net.acme.check-challenge'>,
  ): Promise<T.CheckChallengeRes | null> {
    return this.rpcRequest({
      method: 'net.acme.check-challenge',
      params,
    })
  }

  async checkDns(
    params: Params<'net.gateway.check-dns'>,
  ): Promise<CheckDnsRes> {
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
    return packageResponse(
      await this.rpcRequest({
        method: 'registry.package.get',
        params,
      }),
    )
  }

  async getRegistryPackages(
    params: GetRegistryPackagesReq,
  ): Promise<GetPackagesRes> {
    return packagesResponse(
      await this.rpcRequest({
        method: 'registry.package.get',
        params,
      }),
    )
  }

  // notification

  async getNotifications(
    params: Params<'notification.list'>,
  ): Promise<T.NotificationWithId[]> {
    return this.rpcRequest({ method: 'notification.list', params })
  }

  async deleteNotifications(
    params: Params<'notification.remove'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'notification.remove', params })
  }

  async markSeenNotifications(
    params: Params<'notification.mark-seen'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'notification.mark-seen', params })
  }

  async markSeenAllNotifications(
    params: Params<'notification.mark-seen-before'>,
  ): Promise<null> {
    return this.rpcRequest({
      method: 'notification.mark-seen-before',
      params,
    })
  }

  async markUnseenNotifications(
    params: Params<'notification.mark-unseen'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'notification.mark-unseen', params })
  }

  // proxies

  async addTunnel(params: Params<'net.tunnel.add'>): Promise<string> {
    return this.rpcRequest({ method: 'net.tunnel.add', params })
  }

  async updateTunnel(params: Params<'net.gateway.set-name'>): Promise<null> {
    return this.rpcRequest({ method: 'net.gateway.set-name', params })
  }

  async updateTunnelConfig(params: Params<'net.tunnel.update'>): Promise<null> {
    return this.rpcRequest({ method: 'net.tunnel.update', params })
  }

  async removeTunnel(params: Params<'net.tunnel.remove'>): Promise<null> {
    return this.rpcRequest({ method: 'net.tunnel.remove', params })
  }

  async setDefaultOutbound(
    params: Params<'net.gateway.set-default-outbound'>,
  ): Promise<null> {
    return this.rpcRequest({
      method: 'net.gateway.set-default-outbound',
      params,
    })
  }

  async setServiceOutbound(
    params: Params<'package.set-outbound-gateway'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'package.set-outbound-gateway', params })
  }

  // wifi

  async enableWifi(params: Params<'wifi.set-enabled'>): Promise<null> {
    return this.rpcRequest({ method: 'wifi.set-enabled', params })
  }

  async getWifi(params: {}, timeout?: number): Promise<T.WifiListInfo> {
    return this.rpcRequest({ method: 'wifi.get', params, timeout })
  }

  async setWifiCountry(params: Params<'wifi.country.set'>): Promise<null> {
    return this.rpcRequest({ method: 'wifi.country.set', params })
  }

  async addWifi(params: Params<'wifi.add'>): Promise<null> {
    return this.rpcRequest({ method: 'wifi.add', params })
  }

  async connectWifi(params: Params<'wifi.connect'>): Promise<null> {
    return this.rpcRequest({ method: 'wifi.connect', params })
  }

  async deleteWifi(params: Params<'wifi.remove'>): Promise<null> {
    return this.rpcRequest({ method: 'wifi.remove', params })
  }

  // smtp

  async setSmtp(params: Params<'server.set-smtp'>): Promise<null> {
    return this.rpcRequest({ method: 'server.set-smtp', params })
  }

  async clearSmtp(params: {}): Promise<null> {
    return this.rpcRequest({ method: 'server.clear-smtp', params })
  }

  async testSmtp(params: Params<'server.test-smtp'>): Promise<null> {
    return this.rpcRequest({ method: 'server.test-smtp', params })
  }

  // ssh

  async getSshKeys(params: {}): Promise<T.SshKeyResponse[]> {
    return this.rpcRequest({ method: 'ssh.list', params })
  }

  async addSshKey(params: Params<'ssh.add'>): Promise<T.SshKeyResponse> {
    return this.rpcRequest({ method: 'ssh.add', params })
  }

  async deleteSshKey(params: Params<'ssh.remove'>): Promise<null> {
    return this.rpcRequest({ method: 'ssh.remove', params })
  }

  // backup

  async getBackupTargets(params: {}): Promise<{
    [id: string]: T.BackupTarget
  }> {
    return this.rpcRequest({ method: 'backup.target.list', params })
  }

  async addBackupTarget(
    params: Params<'backup.target.cifs.add'>,
  ): Promise<Record<string, T.BackupTarget>> {
    return this.rpcRequest({ method: 'backup.target.cifs.add', params })
  }

  async updateBackupTarget(
    params: Params<'backup.target.cifs.update'>,
  ): Promise<Record<string, T.BackupTarget>> {
    return this.rpcRequest({ method: 'backup.target.cifs.update', params })
  }

  async removeBackupTarget(
    params: Params<'backup.target.cifs.remove'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'backup.target.cifs.remove', params })
  }

  async deleteLegacyBackup(
    params: Params<'backup.target.delete-legacy'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'backup.target.delete-legacy', params })
  }

  async getBackupInfo(
    params: Params<'backup.target.info'>,
  ): Promise<T.BackupInfo> {
    return this.rpcRequest({ method: 'backup.target.info', params })
  }

  async createBackup(params: Params<'backup.create'>): Promise<null> {
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

  async installPackage(params: Params<'package.install'>): Promise<null> {
    return this.rpcRequest({ method: 'package.install', params })
  }

  async cancelInstallPackage(
    params: Params<'package.cancel-install'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'package.cancel-install', params })
  }

  async getActionInput(
    params: Params<'package.action.get-input'>,
  ): Promise<GetActionInputRes> {
    const result = await this.rpcRequest({
      method: 'package.action.get-input',
      params,
    })
    // The SDK's InputSpec builder owns the opaque specification.
    return result && { ...result, spec: result.spec as IST.InputSpec }
  }

  async runAction(params: Params<'package.action.run'>): Promise<ActionRes> {
    // The handler applies `ActionResult::upcast` before serialization.
    return (await this.rpcRequest({
      method: 'package.action.run',
      params,
    })) as ActionRes
  }

  async clearTask(params: Params<'package.action.clear-task'>): Promise<null> {
    return this.rpcRequest({ method: 'package.action.clear-task', params })
  }

  async restorePackages(
    params: Params<'package.backup.restore'>,
  ): Promise<null> {
    return this.rpcRequest({ method: 'package.backup.restore', params })
  }

  async startPackage(params: Params<'package.start'>): Promise<null> {
    return this.rpcRequest({ method: 'package.start', params })
  }

  async restartPackage(params: Params<'package.restart'>): Promise<null> {
    return this.rpcRequest({ method: 'package.restart', params })
  }

  async stopPackage(params: Params<'package.stop'>): Promise<null> {
    return this.rpcRequest({ method: 'package.stop', params })
  }

  async rebuildPackage(params: Params<'package.rebuild'>): Promise<null> {
    return this.rpcRequest({ method: 'package.rebuild', params })
  }

  async uninstallPackage(params: Params<'package.uninstall'>): Promise<null> {
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

  async removeAcme(params: Params<'net.acme.remove'>): Promise<null> {
    return this.rpcRequest({
      method: 'net.acme.remove',
      params,
    })
  }

  async initAcme(params: Params<'net.acme.init'>): Promise<null> {
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
    params: Params<'server.host.address.domain.public.add'>,
  ): Promise<T.AddPublicDomainRes> {
    return this.rpcRequest({
      method: 'server.host.address.domain.public.add',
      params,
    })
  }

  async osUiRemovePublicDomain(
    params: Params<'server.host.address.domain.public.remove'>,
  ): Promise<null> {
    return this.rpcRequest({
      method: 'server.host.address.domain.public.remove',
      params,
    })
  }

  async osUiAddPrivateDomain(
    params: Params<'server.host.address.domain.private.add'>,
  ): Promise<boolean> {
    return this.rpcRequest({
      method: 'server.host.address.domain.private.add',
      params,
    })
  }

  async osUiRemovePrivateDomain(
    params: Params<'server.host.address.domain.private.remove'>,
  ): Promise<null> {
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

  private async rpcRequest<M extends RPC.RpcMethod<Api>>(
    options: RPCOptions<RPC.RpcParamType<Api, M>, M>,
    urlOverride?: string,
  ): Promise<RPC.RpcReturnType<Api, M>> {
    // Foreign origins must never receive a signature valid at home.
    const res = await this.http.rpcRequest<RPC.RpcReturnType<Api, M>>(
      urlOverride
        ? options
        : {
            ...options,
            headers: {
              ...options.headers,
              ...(await this.authKeys.signRpcHeaders(options)),
            },
          },
      urlOverride,
    )
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
    // Static package assets are authorized; continuation endpoints (uploads,
    // websockets) authenticate by capability URL and need no signature.
    if (opts.url.startsWith('/s9pk')) {
      opts = {
        ...opts,
        headers: {
          ...opts.headers,
          ...(await this.authKeys.signHeader(new Uint8Array(0))),
        },
      }
    }
    const res = await this.http.httpRequest<T>(opts)
    if (res.headers.get('File-Digest')) {
      const digest = res.headers.get('File-Digest')!
      let data: Uint8Array
      if (opts.responseType === 'arrayBuffer') {
        data = Buffer.from(res.body as ArrayBuffer)
      } else if (opts.responseType === 'text') {
        data = Buffer.from(res.body as string)
      } else if ((opts.responseType as string) === 'blob') {
        data = Buffer.from(await (res.body as Blob).arrayBuffer())
      } else {
        console.warn(
          `could not verify File-Digest for responseType ${
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
        console.warn(`Unknown File-Digest algorithm ${alg}`)
      }
    }
    return res.body
  }
}
