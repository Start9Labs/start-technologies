import {
  ExtendedVersion,
  types as T,
  utils,
  VersionRange,
  z,
} from '@start9labs/start-sdk'
import * as net from 'net'
import { Effects } from '../Models/Effects'

import { CallbackHolder } from '../Models/CallbackHolder'
import { asError } from '@start9labs/start-core/util'
const matchRpcError = z.object({
  error: z.object({
    code: z.number(),
    message: z.string(),
    data: z
      .union([
        z.string(),
        z.object({
          details: z.string(),
          debug: z.string().nullable().optional(),
        }),
      ])
      .nullable()
      .optional(),
  }),
})
function testRpcError(v: unknown): v is RpcError {
  return matchRpcError.safeParse(v).success
}
const matchRpcResult = z.object({
  result: z.unknown(),
})
function testRpcResult(v: unknown): v is z.infer<typeof matchRpcResult> {
  return matchRpcResult.safeParse(v).success
}
type RpcError = z.infer<typeof matchRpcError>

const SOCKET_PATH = '/media/startos/rpc/host.sock'
let hostSystemId = 0

export type EffectContext = {
  eventId: string | null
  callbacks?: CallbackHolder
  constRetry?: () => void
}

const rpcRoundFor =
  (eventId: string | null) =>
  <K extends T.EffectMethod | 'clearCallbacks'>(
    method: K,
    params: Record<string, unknown>,
  ) => {
    const id = hostSystemId++
    const client = net.createConnection({ path: SOCKET_PATH }, () => {
      client.write(
        JSON.stringify({
          id,
          method,
          params: { ...params, eventId: eventId ?? undefined },
        }) + '\n',
      )
    })
    let bufs: Buffer[] = []
    return new Promise((resolve, reject) => {
      client.on('data', data => {
        try {
          bufs.push(data)
          if (data.reduce((acc, x) => acc || x == 10, false)) {
            const res: unknown = JSON.parse(
              Buffer.concat(bufs).toString().split('\n')[0],
            )
            if (testRpcError(res)) {
              let message = res.error.message
              console.error(
                'Error in host RPC:',
                utils.asError({ method, params, error: res.error }),
              )
              if (typeof res.error.data === 'string') {
                message += ': ' + res.error.data
                console.error(`Details: ${res.error.data}`)
              } else {
                if (res.error.data?.details) {
                  message += ': ' + res.error.data.details
                  console.error(`Details: ${res.error.data.details}`)
                }
                if (res.error.data?.debug) {
                  message += '\n' + res.error.data.debug
                  console.error(`Debug: ${res.error.data.debug}`)
                }
              }
              reject(new Error(`${message}@${method}`))
            } else if (testRpcResult(res)) {
              resolve(res.result)
            } else {
              reject(new Error(`malformed response ${JSON.stringify(res)}`))
            }
          }
        } catch (error) {
          reject(error)
        }
        client.end()
      })
      client.on('error', error => {
        reject(error)
      })
    })
  }

export function makeEffects(context: EffectContext): Effects {
  const rpcRound = rpcRoundFor(context.eventId)
  const self: Effects = {
    eventId: context.eventId,
    child: name =>
      makeEffects({ ...context, callbacks: context.callbacks?.child(name) }),
    constRetry: context.constRetry,
    isInContext: !!context.callbacks,
    onLeaveContext:
      context.callbacks?.onLeaveContext?.bind(context.callbacks) ||
      (() => {
        console.warn(
          'no context for this effects object',
          new Error().stack?.replace(/^Error/, ''),
        )
      }),
    clearCallbacks(...[options]: Parameters<T.Effects['clearCallbacks']>) {
      return rpcRound('clear-callbacks', {
        ...options,
      }) as ReturnType<T.Effects['clearCallbacks']>
    },
    action: {
      clear(...[options]: Parameters<T.Effects['action']['clear']>) {
        return rpcRound('action.clear', {
          ...options,
        }) as ReturnType<T.Effects['action']['clear']>
      },
      export(...[options]: Parameters<T.Effects['action']['export']>) {
        return rpcRound('action.export', {
          ...options,
        }) as ReturnType<T.Effects['action']['export']>
      },
      getInput(...[options]: Parameters<T.Effects['action']['getInput']>) {
        return rpcRound('action.get-input', {
          ...options,
        }) as ReturnType<T.Effects['action']['getInput']>
      },
      createTask(...[options]: Parameters<T.Effects['action']['createTask']>) {
        return rpcRound('action.create-task', {
          ...options,
        }) as ReturnType<T.Effects['action']['createTask']>
      },
      run(...[options]: Parameters<T.Effects['action']['run']>) {
        return rpcRound('action.run', {
          ...options,
        }) as ReturnType<T.Effects['action']['run']>
      },
      clearTasks(...[options]: Parameters<T.Effects['action']['clearTasks']>) {
        return rpcRound('action.clear-tasks', {
          ...options,
        }) as ReturnType<T.Effects['action']['clearTasks']>
      },
    },
    bind(...[options]: Parameters<T.Effects['bind']>) {
      return rpcRound('bind', {
        ...options,
        stack: new Error().stack,
      }) as ReturnType<T.Effects['bind']>
    },
    bindRange(...[options]: Parameters<T.Effects['bindRange']>) {
      return rpcRound('bind-range', {
        ...options,
        stack: new Error().stack,
      }) as ReturnType<T.Effects['bindRange']>
    },
    clearBindings(...[options]: Parameters<T.Effects['clearBindings']>) {
      return rpcRound('clear-bindings', { ...options }) as ReturnType<
        T.Effects['clearBindings']
      >
    },
    clearServiceInterfaces(
      ...[options]: Parameters<T.Effects['clearServiceInterfaces']>
    ) {
      return rpcRound('clear-service-interfaces', { ...options }) as ReturnType<
        T.Effects['clearServiceInterfaces']
      >
    },
    getInstalledPackages(...[]: Parameters<T.Effects['getInstalledPackages']>) {
      return rpcRound('get-installed-packages', {}) as ReturnType<
        T.Effects['getInstalledPackages']
      >
    },
    getServiceManifest(
      ...[options]: Parameters<T.Effects['getServiceManifest']>
    ) {
      return rpcRound('get-service-manifest', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      }) as ReturnType<T.Effects['getServiceManifest']>
    },
    subcontainer: {
      createFs(options: { imageId: string; name: string }) {
        return rpcRound('subcontainer.create-fs', options) as ReturnType<
          T.Effects['subcontainer']['createFs']
        >
      },
      destroyFs(options: { guid: string }): Promise<null> {
        return rpcRound('subcontainer.destroy-fs', options) as ReturnType<
          T.Effects['subcontainer']['destroyFs']
        >
      },
    },
    exportServiceInterface: ((
      ...[options]: Parameters<Effects['exportServiceInterface']>
    ) => {
      return rpcRound('export-service-interface', options) as ReturnType<
        T.Effects['exportServiceInterface']
      >
    }) as Effects['exportServiceInterface'],
    exportRangeServiceInterface: ((
      ...[options]: Parameters<Effects['exportRangeServiceInterface']>
    ) => {
      return rpcRound('export-range-service-interface', options) as ReturnType<
        T.Effects['exportRangeServiceInterface']
      >
    }) as Effects['exportRangeServiceInterface'],
    getContainerIp(...[options]: Parameters<T.Effects['getContainerIp']>) {
      return rpcRound('get-container-ip', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      }) as ReturnType<T.Effects['getContainerIp']>
    },
    getOsIp(...[]: Parameters<T.Effects['getOsIp']>) {
      return rpcRound('get-os-ip', {}) as ReturnType<T.Effects['getOsIp']>
    },
    getHostInfo: ((...[allOptions]: Parameters<T.Effects['getHostInfo']>) => {
      const options = {
        ...allOptions,
        callback: context.callbacks?.addCallback(allOptions.callback) || null,
      }
      return rpcRound('get-host-info', options) as ReturnType<
        T.Effects['getHostInfo']
      > as any
    }) as Effects['getHostInfo'],
    getServiceInterface(
      ...[options]: Parameters<T.Effects['getServiceInterface']>
    ) {
      return rpcRound('get-service-interface', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      }) as ReturnType<T.Effects['getServiceInterface']>
    },

    getServicePortForward(
      ...[options]: Parameters<T.Effects['getServicePortForward']>
    ) {
      return rpcRound('get-service-port-forward', options) as ReturnType<
        T.Effects['getServicePortForward']
      >
    },
    getSslCertificate(options: Parameters<T.Effects['getSslCertificate']>[0]) {
      return rpcRound('get-ssl-certificate', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      }) as ReturnType<T.Effects['getSslCertificate']>
    },
    getSslKey(options: Parameters<T.Effects['getSslKey']>[0]) {
      return rpcRound('get-ssl-key', options) as ReturnType<
        T.Effects['getSslKey']
      >
    },
    getSystemSmtp(...[options]: Parameters<T.Effects['getSystemSmtp']>) {
      return rpcRound('get-system-smtp', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      }) as ReturnType<T.Effects['getSystemSmtp']>
    },
    getOutboundGateway(
      ...[options]: Parameters<T.Effects['getOutboundGateway']>
    ) {
      return rpcRound('get-outbound-gateway', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      }) as ReturnType<T.Effects['getOutboundGateway']>
    },
    listServiceInterfaces(
      ...[options]: Parameters<T.Effects['listServiceInterfaces']>
    ) {
      return rpcRound('list-service-interfaces', {
        ...options,
        callback: context.callbacks?.addCallback(options.callback) || null,
      }) as ReturnType<T.Effects['listServiceInterfaces']>
    },
    mount(...[options]: Parameters<T.Effects['mount']>) {
      return rpcRound('mount', options) as ReturnType<T.Effects['mount']>
    },
    restart(...[]: Parameters<T.Effects['restart']>) {
      console.log('Restarting service...')
      return rpcRound('restart', {}) as ReturnType<T.Effects['restart']>
    },
    setDependencies(
      dependencies: Parameters<T.Effects['setDependencies']>[0],
    ): ReturnType<T.Effects['setDependencies']> {
      return rpcRound('set-dependencies', dependencies) as ReturnType<
        T.Effects['setDependencies']
      >
    },
    checkDependencies(
      options: Parameters<T.Effects['checkDependencies']>[0],
    ): ReturnType<T.Effects['checkDependencies']> {
      return rpcRound('check-dependencies', options) as ReturnType<
        T.Effects['checkDependencies']
      >
    },
    getDependencies(): ReturnType<T.Effects['getDependencies']> {
      return rpcRound('get-dependencies', {}) as ReturnType<
        T.Effects['getDependencies']
      >
    },
    setHealth(...[options]: Parameters<T.Effects['setHealth']>) {
      return rpcRound('set-health', options) as ReturnType<
        T.Effects['setHealth']
      >
    },
    setBackupProgress(
      ...[options]: Parameters<T.Effects['setBackupProgress']>
    ) {
      return rpcRound('set-backup-progress', options) as ReturnType<
        T.Effects['setBackupProgress']
      >
    },
    setInitProgress(...[options]: Parameters<T.Effects['setInitProgress']>) {
      return rpcRound('set-init-progress', options) as ReturnType<
        T.Effects['setInitProgress']
      >
    },
    notification: {
      create(...[options]: Parameters<T.Effects['notification']['create']>) {
        return rpcRound('notification.create', options) as ReturnType<
          T.Effects['notification']['create']
        >
      },
    },

    getStatus(...[o]: Parameters<T.Effects['getStatus']>) {
      return rpcRound('get-status', {
        ...o,
        callback: context.callbacks?.addCallback(o.callback) || null,
      }) as ReturnType<T.Effects['getStatus']>
    },
    /// DEPRECATED
    setMainStatus(o: { status: 'running' | 'stopped' }): Promise<null> {
      return rpcRound('set-main-status', o) as ReturnType<
        T.Effects['setHealth']
      >
    },

    shutdown(...[]: Parameters<T.Effects['shutdown']>) {
      return rpcRound('shutdown', {}) as ReturnType<T.Effects['shutdown']>
    },
    getDataVersion() {
      return rpcRound('get-data-version', {}) as ReturnType<
        T.Effects['getDataVersion']
      >
    },
    setDataVersion(...[options]: Parameters<T.Effects['setDataVersion']>) {
      return rpcRound('set-data-version', options) as ReturnType<
        T.Effects['setDataVersion']
      >
    },
    plugin: {
      url: {
        register(
          ...[options]: Parameters<T.Effects['plugin']['url']['register']>
        ) {
          return rpcRound('plugin.url.register', options) as ReturnType<
            T.Effects['plugin']['url']['register']
          >
        },
        exportUrl(
          ...[options]: Parameters<T.Effects['plugin']['url']['exportUrl']>
        ) {
          return rpcRound('plugin.url.export-url', options) as ReturnType<
            T.Effects['plugin']['url']['exportUrl']
          >
        },
        clearUrls(
          ...[options]: Parameters<T.Effects['plugin']['url']['clearUrls']>
        ) {
          return rpcRound('plugin.url.clear-urls', options) as ReturnType<
            T.Effects['plugin']['url']['clearUrls']
          >
        },
      },
    },
  }
  if (context.callbacks?.onLeaveContext)
    self.onLeaveContext(() => {
      self.constRetry = undefined
      self.isInContext = false
      self.onLeaveContext = () => {
        console.warn(
          'this effects object is already out of context',
          new Error().stack?.replace(/^Error/, ''),
        )
      }
    })
  return self
}
