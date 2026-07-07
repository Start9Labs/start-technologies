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

  abstract echo(params: T.EchoParams, url: string): Promise<string>

  abstract getState(): Promise<ServerState>

  // db

  abstract subscribeToPatchDB(params: {}): Promise<{
    dump: Dump<DataModel>
    guid: string
  }>

  abstract setDbValue<T>(
    pathArr: Array<string | number>,
    value: T,
  ): Promise<null>

  // auth

  abstract login(params: T.LoginParams): Promise<null>

  abstract logout(params: {}): Promise<null>

  abstract getSessions(params: {}): Promise<T.SessionList>

  abstract killSessions(params: T.KillParams): Promise<null>

  abstract resetPassword(params: T.ResetPasswordParams): Promise<null>

  // diagnostic

  abstract diagnosticGetError(): Promise<DiagnosticErrorRes>
  abstract diagnosticRestart(): Promise<void>
  abstract diagnosticForgetDrive(): Promise<void>
  abstract diagnosticRepairDisk(): Promise<void>
  abstract diagnosticGetLogs(params: T.LogsParams): Promise<T.LogResponse>

  // init

  abstract initFollowProgress(): Promise<T.SetupProgress>

  abstract initFollowLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse>

  // server

  abstract getSystemTime(params: {}): Promise<T.TimeInfo>

  abstract getServerLogs(params: T.LogsParams): Promise<T.LogResponse>

  abstract getKernelLogs(params: T.LogsParams): Promise<T.LogResponse>

  abstract followServerLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse>

  abstract followKernelLogs(
    params: FollowServerLogsReq,
  ): Promise<T.LogFollowResponse>

  abstract followServerMetrics(params: {}): Promise<T.MetricsFollowResponse>

  abstract updateServer(params: {
    registry: string
    targetVersion: string
  }): Promise<'updating' | 'no-updates'>

  abstract restartServer(params: {}): Promise<null>

  abstract shutdownServer(params: {}): Promise<null>

  abstract repairDisk(params: {}): Promise<null>

  abstract toggleKiosk(enable: boolean): Promise<null>

  abstract setHostname(params: T.SetServerHostnameParams): Promise<null>

  abstract setKeyboard(params: FullKeyboard): Promise<null>

  abstract setLanguage(params: SetLanguageParams): Promise<null>

  abstract setDns(params: T.SetStaticDnsParams): Promise<null>

  abstract queryDns(params: T.QueryDnsParams): Promise<T.QueryDnsRes>

  abstract checkPort(params: T.CheckPortParams): Promise<T.CheckPortRes>

  abstract checkPortV6(
    params: T.CheckPortParams,
  ): Promise<T.CheckPortV6Res | null>

  abstract checkDns(params: T.CheckDnsParams): Promise<CheckDnsRes>

  // smtp

  abstract setSmtp(params: T.SmtpValue): Promise<null>

  abstract clearSmtp(params: {}): Promise<null>

  abstract testSmtp(params: T.TestSmtpParams): Promise<null>

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
    params: T.ListNotificationParams,
  ): Promise<T.NotificationWithId[]>

  abstract markSeenNotifications(
    params: T.ModifyNotificationParams,
  ): Promise<null>

  abstract markSeenAllNotifications(
    params: T.ModifyNotificationBeforeParams,
  ): Promise<null>

  abstract markUnseenNotifications(
    params: T.ModifyNotificationParams,
  ): Promise<null>

  abstract deleteNotifications(
    params: T.ModifyNotificationParams,
  ): Promise<null>

  // ** proxies **

  abstract addTunnel(params: T.AddTunnelParams): Promise<{ id: string }>

  abstract updateTunnel(params: T.RenameGatewayParams): Promise<null>

  abstract updateTunnelConfig(params: T.UpdateTunnelParams): Promise<null>

  abstract removeTunnel(params: T.RemoveTunnelParams): Promise<null>

  abstract setDefaultOutbound(params: { gateway: string | null }): Promise<null>

  abstract setServiceOutbound(params: T.SetOutboundGatewayParams): Promise<null>

  // ** domains **

  // wifi

  abstract enableWifi(params: T.SetWifiEnabledParams): Promise<null>

  abstract setWifiCountry(params: T.SetCountryParams): Promise<null>

  abstract getWifi(params: {}, timeout: number): Promise<T.WifiListInfo>

  abstract addWifi(params: T.WifiAddParams): Promise<null>

  abstract connectWifi(params: T.WifiSsidParams): Promise<null>

  abstract deleteWifi(params: T.WifiSsidParams): Promise<null>

  // ssh

  abstract getSshKeys(params: {}): Promise<T.SshKeyResponse[]>

  abstract addSshKey(params: T.SshAddParams): Promise<T.SshKeyResponse>

  abstract deleteSshKey(params: T.SshDeleteParams): Promise<null>

  // backup

  abstract getBackupTargets(params: {}): Promise<{
    [id: string]: T.BackupTarget
  }>

  abstract addBackupTarget(
    params: T.CifsAddParams,
  ): Promise<{ [id: string]: CifsBackupTarget }>

  abstract updateBackupTarget(
    params: T.CifsUpdateParams,
  ): Promise<{ [id: string]: CifsBackupTarget }>

  abstract removeBackupTarget(params: T.CifsRemoveParams): Promise<null>

  abstract deleteLegacyBackup(params: T.DeleteLegacyParams): Promise<null>

  abstract getBackupInfo(params: T.InfoParams): Promise<T.BackupInfo>

  abstract createBackup(params: T.BackupParams): Promise<null>

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

  abstract installPackage(params: T.InstallParams): Promise<null>

  abstract cancelInstallPackage(params: T.CancelInstallParams): Promise<null>

  abstract getActionInput(
    params: T.GetActionInputParams,
  ): Promise<GetActionInputRes>

  abstract runAction(params: T.RunActionParams): Promise<ActionRes>

  abstract clearTask(params: T.ClearTaskParams): Promise<null>

  abstract restorePackages(params: T.RestorePackageParams): Promise<null>

  abstract startPackage(params: T.ControlParams): Promise<null>

  abstract restartPackage(params: T.ControlParams): Promise<null>

  abstract stopPackage(params: T.ControlParams): Promise<null>

  abstract rebuildPackage(params: T.RebuildParams): Promise<null>

  abstract uninstallPackage(params: T.UninstallParams): Promise<null>

  abstract sideloadPackage(): Promise<T.SideloadResponse>

  // @TODO 041

  // ** service outbound proxy **

  // abstract setServiceOutboundProxy(
  //   params: RR.SetServiceOutboundTunnelReq,
  // ): Promise<RR.SetServiceOutboundTunnelRes>

  abstract initAcme(params: T.InitAcmeParams): Promise<null>

  abstract removeAcme(params: T.RemoveAcmeParams): Promise<null>

  abstract serverBindingSetAddressEnabled(
    params: ServerBindingSetAddressEnabledReq,
  ): Promise<null>

  abstract osUiAddPublicDomain(
    params: T.AddPublicDomainParams,
  ): Promise<T.AddPublicDomainRes>

  abstract osUiRemovePublicDomain(params: T.RemoveDomainParams): Promise<null>

  abstract osUiAddPrivateDomain(
    params: T.AddPrivateDomainParams,
  ): Promise<boolean>

  abstract osUiRemovePrivateDomain(params: T.RemoveDomainParams): Promise<null>

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
