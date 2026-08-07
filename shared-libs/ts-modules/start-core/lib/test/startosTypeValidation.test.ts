import { Effects } from '../types'
import {
  CheckDependenciesParam,
  ClearTasksParams,
  ClearActionsParams,
  ClearBindingsParams,
  ClearCallbacksParams,
  ClearServiceInterfacesParams,
  GetActionInputParams,
  GetContainerIpParams,
  GetStatusParams,
  CreateTaskParams,
  EffectsRunActionParams,
  SetDataVersionParams,
  SetMainStatus,
  GetServiceManifestParams,
} from '.././osBindings'
import { CreateSubcontainerFsParams } from '.././osBindings'
import { DestroySubcontainerFsParams } from '.././osBindings'
import { BindParams } from '.././osBindings'
import { BindRangeParams } from '.././osBindings'
import { RetireHostParams } from '.././osBindings'
import { RetireBindingParams } from '.././osBindings'
import { GetHostInfoParams } from '.././osBindings'
import { CreateNotificationParams } from '.././osBindings'
import { SetHealth, SetBackupProgress, SetInitProgress } from '.././osBindings'
import { GetSslCertificateParams } from '.././osBindings'
import { GetSslKeyParams } from '.././osBindings'
import { GetServiceInterfaceParams } from '.././osBindings'
import { SetDependenciesParams } from '.././osBindings'
import { GetSystemSmtpParams } from '.././osBindings'
import { GetOutboundGatewayParams } from '.././osBindings'
import { GetServicePortForwardParams } from '.././osBindings'
import { ExportServiceInterfaceParams } from '.././osBindings'
import { ExportRangeServiceInterfaceParams } from '.././osBindings'
import { ListServiceInterfacesParams } from '.././osBindings'
import { ExportActionParams } from '.././osBindings'
import { MountParams } from '.././osBindings'
import { UrlPluginRegisterParams } from '.././osBindings'
import { UrlPluginExportUrlParams } from '.././osBindings'
import { UrlPluginClearUrlsParams } from '.././osBindings'
import { StringObject } from '../util'
import { ExtendedVersion, VersionRange } from '../exver'
function typeEquality<ExpectedType>(_a: ExpectedType) {}

type WithCallback<T> = Omit<T, 'callback'> & { callback: () => void }

type EffectsTypeChecker<T extends StringObject = Effects> = {
  [K in keyof T]: T[K] extends (args: infer A) => any
    ? A
    : T[K] extends StringObject
      ? EffectsTypeChecker<T[K]>
      : never
}

describe('startosTypeValidation ', () => {
  test(`checking the params match`, () => {
    typeEquality<EffectsTypeChecker>({
      eventId: {} as never,
      child: '',
      isInContext: {} as never,
      onLeaveContext: () => {},
      clearCallbacks: {} as ClearCallbacksParams,
      action: {
        clear: {} as ClearActionsParams,
        export: {} as ExportActionParams,
        getInput: {} as GetActionInputParams,
        run: {} as EffectsRunActionParams,
        createTask: {} as CreateTaskParams,
        clearTasks: {} as ClearTasksParams,
      },
      subcontainer: {
        createFs: {} as CreateSubcontainerFsParams,
        destroyFs: {} as DestroySubcontainerFsParams,
      },
      clearBindings: {} as ClearBindingsParams,
      bind: {} as BindParams,
      bindRange: {} as BindRangeParams,
      retireHost: {} as RetireHostParams,
      retireBinding: {} as RetireBindingParams,
      getHostInfo: {} as WithCallback<GetHostInfoParams>,
      restart: undefined,
      shutdown: undefined,
      setDataVersion: {} as SetDataVersionParams,
      getDataVersion: undefined,
      setHealth: {} as SetHealth,
      setBackupProgress: {} as SetBackupProgress,
      setInitProgress: {} as SetInitProgress,
      notification: {
        create: {} as CreateNotificationParams,
      },
      getSslCertificate: {} as WithCallback<GetSslCertificateParams>,
      getSslKey: {} as GetSslKeyParams,
      getServiceInterface: {} as WithCallback<GetServiceInterfaceParams>,
      setDependencies: {} as SetDependenciesParams,
      getInstalledPackages: undefined,
      getServiceManifest: {} as WithCallback<GetServiceManifestParams>,
      getSystemSmtp: {} as WithCallback<GetSystemSmtpParams>,
      getContainerIp: {} as WithCallback<GetContainerIpParams>,
      getOutboundGateway: {} as WithCallback<GetOutboundGatewayParams>,
      getOsIp: undefined,
      getServicePortForward: {} as GetServicePortForwardParams,
      clearServiceInterfaces: {} as ClearServiceInterfacesParams,
      exportServiceInterface: {} as ExportServiceInterfaceParams,
      exportRangeServiceInterface: {} as ExportRangeServiceInterfaceParams,
      listServiceInterfaces: {} as WithCallback<ListServiceInterfacesParams>,
      mount: {} as MountParams,
      checkDependencies: {} as CheckDependenciesParam,
      getDependencies: undefined,
      getStatus: {} as WithCallback<GetStatusParams>,
      setMainStatus: {} as SetMainStatus,
      plugin: {
        url: {
          register: {} as UrlPluginRegisterParams,
          exportUrl: {} as UrlPluginExportUrlParams,
          clearUrls: {} as UrlPluginClearUrlsParams,
        },
      },
    })
  })
})
