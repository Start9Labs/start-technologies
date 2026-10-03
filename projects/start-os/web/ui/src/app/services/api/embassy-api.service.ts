import type { Api, Params } from './api.types'
import { FullKeyboard, SetLanguageParams } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { GetPackageRes, GetPackagesRes } from '@start9labs/marketplace'
import { Dump } from 'patch-db-client'
import { WebSocketSubject } from 'rxjs/webSocket'
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

export abstract class ApiService {
  // http

  // for uploading files
  abstract uploadFile(guid: string, body: Blob): Promise<void>

  // for getting static files: ex license
  abstract getStatic(
    urls: string[],
    params: Record<string, string | number>,
  ): Promise<string>

  // websocket

  abstract openWebsocket$<T>(
    guid: string,
    config?: WebsocketConfig<T>,
  ): WebSocketSubject<T>

  // state

  abstract echo(params: Params<'echo'>, url: string): Promise<string>

  abstract getState(): Promise<ServerState>

  // db

  abstract subscribeToPatchDB(): Promise<{
    dump: Dump<DataModel>
    guid: string
  }>

  abstract setDbValue<T>(
    pathArr: Array<string | number>,
    value: T,
  ): Promise<null>

  // auth

  abstract login(params: Params<'auth.login'>): Promise<null>

  abstract logout(params: {}): Promise<null>

  abstract getSessions(params: {}): Promise<T.SessionList>

  abstract killSessions(params: Params<'auth.session.kill'>): Promise<null>

  abstract resetPassword(params: Params<'auth.reset-password'>): Promise<null>

  // diagnostic

  abstract diagnosticGetError(): Promise<DiagnosticErrorRes>
  abstract diagnosticRestart(): Promise<null>
  abstract diagnosticForgetDrive(): Promise<null>
  abstract diagnosticRepairDisk(): Promise<null>
  abstract diagnosticGetLogs(
    params: Params<'diagnostic.logs'>,
  ): Promise<T.LogResponse>

  // init

  abstract initFollowProgress(): Promise<T.SetupProgress>

  abstract initFollowLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse>

  // server

  abstract getSystemTime(params: {}): Promise<T.TimeInfo>

  abstract getServerLogs(params: Params<'server.logs'>): Promise<T.LogResponse>

  abstract getKernelLogs(
    params: Params<'server.kernel-logs'>,
  ): Promise<T.LogResponse>

  abstract followServerLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse>

  abstract followKernelLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse>

  abstract followServerMetrics(params: {}): Promise<T.MetricsFollowResponse>

  abstract updateServer(
    params: Params<'server.update'>,
  ): Promise<T.UpdateSystemRes>

  abstract restartServer(params: {}): Promise<null>

  abstract shutdownServer(params: {}): Promise<null>

  abstract repairDisk(params: {}): Promise<null>

  abstract toggleKiosk(enable: boolean): Promise<null>

  abstract setHostname(params: Params<'server.set-hostname'>): Promise<null>

  abstract setKeyboard(params: FullKeyboard): Promise<null>

  abstract setLanguage(params: SetLanguageParams): Promise<null>

  abstract setDns(params: Params<'net.dns.set-static'>): Promise<null>

  abstract queryDns(params: Params<'net.dns.query'>): Promise<T.QueryDnsRes>

  abstract checkPort(
    params: Params<'net.gateway.check-port'>,
  ): Promise<T.CheckPortRes>

  abstract checkPortV6(
    params: Params<'net.gateway.check-port-v6'>,
  ): Promise<T.CheckPortV6Res | null>

  abstract checkChallenge(
    params: Params<'net.acme.check-challenge'>,
  ): Promise<T.CheckChallengeRes | null>

  abstract checkDns(
    params: Params<'net.gateway.check-dns'>,
  ): Promise<CheckDnsRes>

  // smtp

  abstract setSmtp(params: Params<'server.set-smtp'>): Promise<null>

  abstract clearSmtp(params: {}): Promise<null>

  abstract testSmtp(params: Params<'server.test-smtp'>): Promise<null>

  // marketplace URLs

  abstract checkOSUpdate(params: {
    registry: string
    serverId: string
  }): Promise<T.OsVersionInfoMap>

  abstract getRegistryInfo(params: {
    registry: string
  }): Promise<T.RegistryInfo>

  abstract getRegistryPackage(
    params: GetRegistryPackageReq,
  ): Promise<GetPackageRes>

  abstract getRegistryPackages(
    params: GetRegistryPackagesReq,
  ): Promise<GetPackagesRes>

  // notification

  abstract getNotifications(
    params: Params<'notification.list'>,
  ): Promise<T.NotificationWithId[]>

  abstract markSeenNotifications(
    params: Params<'notification.mark-seen'>,
  ): Promise<null>

  abstract markSeenAllNotifications(
    params: Params<'notification.mark-seen-before'>,
  ): Promise<null>

  abstract markUnseenNotifications(
    params: Params<'notification.mark-unseen'>,
  ): Promise<null>

  abstract deleteNotifications(
    params: Params<'notification.remove'>,
  ): Promise<null>

  // ** proxies **

  abstract addTunnel(params: Params<'net.tunnel.add'>): Promise<string>

  abstract updateTunnel(params: Params<'net.gateway.set-name'>): Promise<null>

  abstract updateTunnelConfig(
    params: Params<'net.tunnel.update'>,
  ): Promise<null>

  abstract removeTunnel(params: Params<'net.tunnel.remove'>): Promise<null>

  abstract setDefaultOutbound(params: { gateway: string | null }): Promise<null>

  abstract setServiceOutbound(
    params: Params<'package.set-outbound-gateway'>,
  ): Promise<null>

  // ** domains **

  // wifi

  abstract enableWifi(params: Params<'wifi.set-enabled'>): Promise<null>

  abstract setWifiCountry(params: Params<'wifi.country.set'>): Promise<null>

  abstract getWifi(params: {}, timeout: number): Promise<T.WifiListInfo>

  abstract addWifi(params: Params<'wifi.add'>): Promise<null>

  abstract connectWifi(params: Params<'wifi.connect'>): Promise<null>

  abstract deleteWifi(params: Params<'wifi.remove'>): Promise<null>

  // ssh

  abstract getSshKeys(params: {}): Promise<T.SshKeyResponse[]>

  abstract addSshKey(params: Params<'ssh.add'>): Promise<T.SshKeyResponse>

  abstract deleteSshKey(params: Params<'ssh.remove'>): Promise<null>

  // backup

  abstract getBackupTargets(params: {}): Promise<{
    [id: string]: T.BackupTarget
  }>

  abstract addBackupTarget(
    params: Params<'backup.target.cifs.add'>,
  ): Promise<Record<string, T.BackupTarget>>

  abstract updateBackupTarget(
    params: Params<'backup.target.cifs.update'>,
  ): Promise<Record<string, T.BackupTarget>>

  abstract removeBackupTarget(
    params: Params<'backup.target.cifs.remove'>,
  ): Promise<null>

  abstract deleteLegacyBackup(
    params: Params<'backup.target.delete-legacy'>,
  ): Promise<null>

  abstract getBackupInfo(
    params: Params<'backup.target.info'>,
  ): Promise<T.BackupInfo>

  abstract createBackup(params: Params<'backup.create'>): Promise<null>

  // @TODO 041

  // ** automated backups **

  // abstract addBackupTarget(
  //   type: BackupTargetType,
  //   params:
  //     | RR.AddCifsBackupTargetReq
  //     | RR.AddCloudBackupTargetReq
  //     | RR.AddDiskBackupTargetReq,
  // ): Promise<RR.AddBackupTargetRes>

  // abstract updateBackupTarget(
  //   type: BackupTargetType,
  //   params:
  //     | RR.UpdateCifsBackupTargetReq
  //     | RR.UpdateCloudBackupTargetReq
  //     | RR.UpdateDiskBackupTargetReq,
  // ): Promise<RR.UpdateBackupTargetRes>

  // abstract removeBackupTarget(
  //   params: RR.RemoveBackupTargetReq,
  // ): Promise<RR.RemoveBackupTargetRes>

  // abstract getBackupJobs(
  //   params: RR.GetBackupJobsReq,
  // ): Promise<RR.GetBackupJobsRes>

  // abstract createBackupJob(
  //   params: RR.CreateBackupJobReq,
  // ): Promise<RR.CreateBackupJobRes>

  // abstract updateBackupJob(
  //   params: RR.UpdateBackupJobReq,
  // ): Promise<RR.UpdateBackupJobRes>

  // abstract deleteBackupJob(
  //   params: RR.DeleteBackupJobReq,
  // ): Promise<RR.DeleteBackupJobRes>

  // abstract getBackupRuns(
  //   params: RR.GetBackupRunsReq,
  // ): Promise<RR.GetBackupRunsRes>

  // abstract deleteBackupRuns(
  //   params: RR.DeleteBackupRunsReq,
  // ): Promise<RR.DeleteBackupRunsRes>

  // package

  abstract getPackageLogs(params: GetPackageLogsReq): Promise<T.LogResponse>

  abstract followPackageLogs(
    params: FollowPackageLogsReq,
  ): Promise<T.LogFollowResponse>

  abstract installPackage(params: Params<'package.install'>): Promise<null>

  abstract cancelInstallPackage(
    params: Params<'package.cancel-install'>,
  ): Promise<null>

  abstract getActionInput(
    params: Params<'package.action.get-input'>,
  ): Promise<GetActionInputRes>

  abstract runAction(params: Params<'package.action.run'>): Promise<ActionRes>

  abstract clearTask(params: Params<'package.action.clear-task'>): Promise<null>

  abstract restorePackages(
    params: Params<'package.backup.restore'>,
  ): Promise<null>

  abstract startPackage(params: Params<'package.start'>): Promise<null>

  abstract restartPackage(params: Params<'package.restart'>): Promise<null>

  abstract stopPackage(params: Params<'package.stop'>): Promise<null>

  abstract rebuildPackage(params: Params<'package.rebuild'>): Promise<null>

  abstract uninstallPackage(params: Params<'package.uninstall'>): Promise<null>

  abstract sideloadPackage(): Promise<T.SideloadResponse>

  // @TODO 041

  // ** service outbound proxy **

  // abstract setServiceOutboundProxy(
  //   params: RR.SetServiceOutboundTunnelReq,
  // ): Promise<RR.SetServiceOutboundTunnelRes>

  abstract initAcme(params: Params<'net.acme.init'>): Promise<null>

  abstract removeAcme(params: Params<'net.acme.remove'>): Promise<null>

  abstract serverBindingSetAddressEnabled(
    params: ServerBindingSetAddressEnabledReq,
  ): Promise<null>

  abstract osUiAddPublicDomain(
    params: Params<'server.host.address.domain.public.add'>,
  ): Promise<T.AddPublicDomainRes>

  abstract osUiRemovePublicDomain(
    params: Params<'server.host.address.domain.public.remove'>,
  ): Promise<null>

  abstract osUiAddPrivateDomain(
    params: Params<'server.host.address.domain.private.add'>,
  ): Promise<boolean>

  abstract osUiRemovePrivateDomain(
    params: Params<'server.host.address.domain.private.remove'>,
  ): Promise<null>

  abstract pkgBindingSetAddressEnabled(
    params: PkgBindingSetAddressEnabledReq,
  ): Promise<null>

  abstract pkgBindingSetRangeAddressEnabled(
    params: PkgBindingSetAddressEnabledReq,
  ): Promise<null>

  abstract serverBindingSetGuaWan(
    params: ServerBindingSetGuaWanReq,
  ): Promise<null>

  abstract pkgBindingSetGuaWan(params: PkgBindingSetGuaWanReq): Promise<null>

  abstract pkgAddPublicDomain(
    params: PkgAddPublicDomainReq,
  ): Promise<T.AddPublicDomainRes>

  abstract pkgRemovePublicDomain(
    params: PkgRemovePublicDomainReq,
  ): Promise<null>

  abstract pkgAddPrivateDomain(params: PkgAddPrivateDomainReq): Promise<boolean>

  abstract pkgRemovePrivateDomain(
    params: PkgRemovePrivateDomainReq,
  ): Promise<null>
}
